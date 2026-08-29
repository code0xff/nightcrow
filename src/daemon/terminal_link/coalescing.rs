use super::TerminalMessage;
use std::collections::VecDeque;

pub(super) fn fits(
    tail: Option<&TerminalMessage>,
    incoming: &TerminalMessage,
    max_bytes: usize,
) -> bool {
    match (tail, incoming) {
        (
            Some(TerminalMessage::Output {
                pane: queued_pane,
                data: queued_data,
            }),
            TerminalMessage::Output { pane, data },
        ) => {
            queued_pane == pane
                && queued_data
                    .len()
                    .checked_add(data.len())
                    .is_some_and(|total| total <= max_bytes)
        }
        _ => false,
    }
}

pub(super) fn append_to_fitting_tail(
    inbox: &mut VecDeque<TerminalMessage>,
    message: TerminalMessage,
) {
    let TerminalMessage::Output { data, .. } = message else {
        unreachable!("coalescing only applies to output messages");
    };
    let Some(TerminalMessage::Output {
        data: queued_data, ..
    }) = inbox.back_mut()
    else {
        unreachable!("coalescing requires a queued output message");
    };
    queued_data.extend(data);
}
