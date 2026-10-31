use crate::application::draw_backend::QuietBackend;
use ratatui::backend::{Backend, ClearType, WindowSize};
use ratatui::buffer::Cell;
use ratatui::layout::{Position, Size};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Paragraph;
use ratatui::{Terminal, backend::CrosstermBackend};
use std::cell::{Cell as StateCell, RefCell};
use std::io::{self, Write};
use std::rc::Rc;

#[derive(Default)]
struct WriterState {
    bytes: Vec<u8>,
    writes: usize,
    flushes: usize,
    fail_writes: usize,
    fail_flushes: usize,
}

#[derive(Clone, Default)]
struct RecordingWriter(Rc<RefCell<WriterState>>);

impl Write for RecordingWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut state = self.0.borrow_mut();
        if state.fail_writes > 0 {
            state.fail_writes -= 1;
            return Err(io::Error::other("injected write failure"));
        }
        state.writes += 1;
        state.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut state = self.0.borrow_mut();
        if state.fail_flushes > 0 {
            state.fail_flushes -= 1;
            return Err(io::Error::other("injected flush failure"));
        }
        state.flushes += 1;
        Ok(())
    }
}

struct FixedCrossterm {
    inner: CrosstermBackend<RecordingWriter>,
    size: Rc<StateCell<Size>>,
    cursor: Position,
}

impl Backend for FixedCrossterm {
    type Error = io::Error;

    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        self.inner.draw(content)
    }

    fn append_lines(&mut self, n: u16) -> io::Result<()> {
        self.inner.append_lines(n)
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        self.inner.hide_cursor()
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        self.inner.show_cursor()
    }

    fn get_cursor_position(&mut self) -> io::Result<Position> {
        Ok(self.cursor)
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
        let position = position.into();
        self.inner.set_cursor_position(position)?;
        self.cursor = position;
        Ok(())
    }

    fn clear(&mut self) -> io::Result<()> {
        self.inner.clear()
    }

    fn clear_region(&mut self, clear_type: ClearType) -> io::Result<()> {
        self.inner.clear_region(clear_type)
    }

    fn size(&self) -> io::Result<Size> {
        Ok(self.size.get())
    }

    fn window_size(&mut self) -> io::Result<WindowSize> {
        Ok(WindowSize {
            columns_rows: self.size.get(),
            pixels: Size::default(),
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        Backend::flush(&mut self.inner)
    }
}

type TestTerminal = Terminal<QuietBackend<FixedCrossterm>>;

fn terminal() -> (TestTerminal, RecordingWriter, Rc<StateCell<Size>>) {
    let writer = RecordingWriter::default();
    let size = Rc::new(StateCell::new(Size::new(80, 24)));
    let backend = FixedCrossterm {
        inner: CrosstermBackend::new(writer.clone()),
        size: size.clone(),
        cursor: Position { x: 0, y: 0 },
    };
    (
        Terminal::new(QuietBackend::new(backend)).unwrap(),
        writer,
        size,
    )
}

fn draw(
    terminal: &mut TestTerminal,
    text: &str,
    cursor: Option<Position>,
    style: Style,
) -> io::Result<()> {
    terminal
        .draw(|frame| {
            frame.render_widget(Paragraph::new(text).style(style), frame.area());
            if let Some(cursor) = cursor {
                frame.set_cursor_position(cursor);
            }
        })
        .map(|_| ())
}

fn reset(writer: &RecordingWriter) {
    let mut state = writer.0.borrow_mut();
    state.bytes.clear();
    state.writes = 0;
    state.flushes = 0;
    state.fail_flushes = 0;
}

fn captured_bytes(writer: &RecordingWriter) -> Vec<u8> {
    writer.0.borrow().bytes.clone()
}

#[test]
fn identical_frames_emit_no_bytes_or_backend_flushes() {
    let (mut terminal, writer, _) = terminal();
    draw(&mut terminal, "nightcrow", None, Style::default()).unwrap();
    let baseline = writer.0.borrow();
    assert!(!baseline.bytes.is_empty());
    assert!(baseline.writes > 0);
    assert!(baseline.flushes > 0);
    eprintln!(
        "initial frame: {} bytes, {} writes, {} flushes",
        baseline.bytes.len(),
        baseline.writes,
        baseline.flushes
    );
    drop(baseline);
    reset(&writer);

    draw(&mut terminal, "nightcrow", None, Style::default()).unwrap();

    let state = writer.0.borrow();
    assert!(state.bytes.is_empty());
    assert_eq!(state.writes, 0);
    assert_eq!(state.flushes, 0);
}

#[test]
fn content_cursor_visibility_and_position_changes_emit_only_when_needed() {
    let (mut terminal, writer, _) = terminal();
    draw(&mut terminal, "same", None, Style::default()).unwrap();
    reset(&writer);

    let cursor = Position { x: 79, y: 23 };
    draw(&mut terminal, "same", Some(cursor), Style::default()).unwrap();
    assert!(
        !captured_bytes(&writer).is_empty(),
        "cursor-only change is drawn"
    );
    reset(&writer);
    draw(&mut terminal, "same", Some(cursor), Style::default()).unwrap();
    assert!(captured_bytes(&writer).is_empty(), "same cursor is cached");

    reset(&writer);
    draw(
        &mut terminal,
        "changed",
        Some(cursor),
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
    )
    .unwrap();
    let bytes = captured_bytes(&writer);
    assert!(!bytes.is_empty());
    assert!(bytes.windows(8).any(|part| part == b"\x1b[24;80H"));

    reset(&writer);
    draw(&mut terminal, "changed", None, Style::default()).unwrap();
    assert!(
        !captured_bytes(&writer).is_empty(),
        "cursor hide is emitted once"
    );
    reset(&writer);
    draw(&mut terminal, "changed", None, Style::default()).unwrap();
    assert!(
        captured_bytes(&writer).is_empty(),
        "repeated hide is skipped"
    );
}

#[test]
fn clear_redraw_resize_and_failed_writes_are_not_cached() {
    let (mut terminal, writer, size) = terminal();
    draw(&mut terminal, "before", None, Style::default()).unwrap();
    reset(&writer);

    terminal.clear().unwrap();
    assert!(!writer.0.borrow().bytes.is_empty());
    reset(&writer);
    draw(&mut terminal, "before", None, Style::default()).unwrap();
    assert!(
        !writer.0.borrow().bytes.is_empty(),
        "clear forces a full redraw"
    );

    reset(&writer);
    size.set(Size::new(40, 10));
    draw(&mut terminal, "before", None, Style::default()).unwrap();
    assert!(
        !writer.0.borrow().bytes.is_empty(),
        "resize redraws the cells"
    );

    reset(&writer);
    writer.0.borrow_mut().fail_writes = 1;
    assert!(draw(&mut terminal, "after", None, Style::default()).is_err());
    reset(&writer);
    draw(&mut terminal, "after", None, Style::default()).unwrap();
    assert!(
        !writer.0.borrow().bytes.is_empty(),
        "failed drawing is retried"
    );
}

#[test]
fn failed_cursor_visibility_is_retried() {
    let (mut terminal, writer, _) = terminal();
    draw(&mut terminal, "same", None, Style::default()).unwrap();
    reset(&writer);
    writer.0.borrow_mut().fail_writes = 1;
    let cursor = Position { x: 2, y: 1 };
    assert!(draw(&mut terminal, "same", Some(cursor), Style::default()).is_err());

    reset(&writer);
    draw(&mut terminal, "same", Some(cursor), Style::default()).unwrap();
    let bytes = captured_bytes(&writer);
    assert!(bytes.windows(6).any(|part| part == b"\x1b[?25h"));
    assert!(bytes.windows(6).any(|part| part == b"\x1b[2;3H"));
}

#[test]
fn a_failed_final_flush_invalidates_cursor_visibility_for_retry() {
    let (mut terminal, writer, _) = terminal();
    draw(&mut terminal, "before", None, Style::default()).unwrap();
    reset(&writer);
    writer.0.borrow_mut().fail_flushes = 1;

    assert!(draw(&mut terminal, "after", None, Style::default()).is_err());

    reset(&writer);
    draw(&mut terminal, "after", None, Style::default()).unwrap();
    let bytes = captured_bytes(&writer);
    assert!(bytes.windows(6).any(|part| part == b"\x1b[?25l"));
}
