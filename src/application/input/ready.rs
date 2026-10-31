use crossterm::event::Event;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InputControl {
    Continue,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InputProgress {
    pub(crate) processed: usize,
    pub(crate) quit: bool,
}

/// Dispatch one source event per tick, preserving crossterm's Unix event
/// boundary and one Windows paste-coalesced batch.
pub(crate) fn process_ready_input(
    mut read_ready: impl FnMut() -> anyhow::Result<Option<Event>>,
    mut expand: impl FnMut(Event) -> anyhow::Result<Vec<Event>>,
    mut dispatch: impl FnMut(Event) -> anyhow::Result<InputControl>,
) -> anyhow::Result<InputProgress> {
    let Some(first) = read_ready()? else {
        return Ok(InputProgress {
            processed: 0,
            quit: false,
        });
    };
    let events = expand(first)?;
    anyhow::ensure!(
        !events.is_empty(),
        "input expansion discarded a consumed event"
    );

    let mut processed = 0;
    for event in events {
        processed += 1;
        if dispatch(event)? == InputControl::Quit {
            return Ok(InputProgress {
                processed,
                quit: true,
            });
        }
    }
    Ok(InputProgress {
        processed,
        quit: false,
    })
}

#[cfg(test)]
#[path = "ready_tests.rs"]
mod tests;
