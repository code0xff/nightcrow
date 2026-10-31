use ratatui::backend::{Backend, ClearType, WindowSize};
use ratatui::layout::{Position, Size};

/// Avoids emitting terminal commands when Ratatui's frame diff is empty or
/// when the physical cursor already has the requested state.
pub(crate) struct QuietBackend<B> {
    inner: B,
    cursor_visible: Option<bool>,
    cursor_position: Option<Position>,
    output_pending: bool,
}

impl<B> QuietBackend<B> {
    pub(crate) fn new(inner: B) -> Self {
        Self {
            inner,
            cursor_visible: None,
            cursor_position: None,
            output_pending: false,
        }
    }
}

impl<B: Backend> Backend for QuietBackend<B> {
    type Error = B::Error;

    fn draw<'a, I>(&mut self, content: I) -> Result<(), Self::Error>
    where
        I: Iterator<Item = (u16, u16, &'a ratatui::buffer::Cell)>,
    {
        let mut content = content.peekable();
        if content.peek().is_none() {
            return Ok(());
        }
        self.cursor_position = None;
        self.output_pending = true;
        self.inner.draw(content)
    }

    fn append_lines(&mut self, n: u16) -> Result<(), Self::Error> {
        if n == 0 {
            return Ok(());
        }
        self.invalidate_physical_state();
        self.output_pending = true;
        self.inner.append_lines(n)
    }

    fn hide_cursor(&mut self) -> Result<(), Self::Error> {
        self.set_cursor_visibility(false)
    }

    fn show_cursor(&mut self) -> Result<(), Self::Error> {
        self.set_cursor_visibility(true)
    }

    fn get_cursor_position(&mut self) -> Result<Position, Self::Error> {
        let position = self.inner.get_cursor_position()?;
        self.cursor_position = Some(position);
        Ok(position)
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> Result<(), Self::Error> {
        let position = position.into();
        if self.cursor_position == Some(position) {
            return Ok(());
        }
        self.cursor_position = None;
        self.output_pending = true;
        self.inner.set_cursor_position(position)?;
        self.cursor_position = Some(position);
        Ok(())
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        self.invalidate_physical_state();
        self.output_pending = true;
        self.inner.clear()
    }

    fn clear_region(&mut self, clear_type: ClearType) -> Result<(), Self::Error> {
        self.invalidate_physical_state();
        self.output_pending = true;
        self.inner.clear_region(clear_type)
    }

    fn size(&self) -> Result<Size, Self::Error> {
        self.inner.size()
    }

    fn window_size(&mut self) -> Result<WindowSize, Self::Error> {
        self.inner.window_size()
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        if self.output_pending {
            match self.inner.flush() {
                Ok(()) => self.output_pending = false,
                Err(error) => {
                    self.invalidate_physical_state();
                    return Err(error);
                }
            }
        }
        Ok(())
    }
}

impl<B> QuietBackend<B> {
    fn set_cursor_visibility(&mut self, visible: bool) -> Result<(), B::Error>
    where
        B: Backend,
    {
        if self.cursor_visible == Some(visible) {
            return Ok(());
        }
        self.cursor_visible = None;
        self.output_pending = true;
        if visible {
            self.inner.show_cursor()?;
        } else {
            self.inner.hide_cursor()?;
        }
        self.cursor_visible = Some(visible);
        Ok(())
    }

    fn invalidate_physical_state(&mut self) {
        self.cursor_position = None;
        self.cursor_visible = None;
    }
}
