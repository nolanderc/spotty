pub struct Screen {
    pub title: String,

    pub grid: crate::grid::CharacterGrid,
    pub alternate_grid: crate::grid::CharacterGrid,

    pub cursor: crate::grid::Position,
    pub saved_cursor: crate::grid::Position,
    pub cursor_style: crate::tty::control_code::CursorStyle,
    pub cursor_color: crate::color::Color,

    pub style: crate::tty::control_code::CharacterStyles,
    pub foreground: crate::color::Color,
    pub background: crate::color::Color,

    pub scrolling_region: std::ops::Range<u16>,

    pub behaviours: Behaviours,

    /// Output from the shell that hasn't been parsed yet due to needing more bytes.
    residual_input: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Behaviours {
    pub show_cursor: bool,
    pub alternate_buffer: bool,
    pub bracketed_paste: bool,
}

impl Default for Behaviours {
    fn default() -> Self {
        Behaviours {
            show_cursor: true,
            alternate_buffer: false,
            bracketed_paste: false,
        }
    }
}

impl Screen {
    pub fn new(grid_size: [u16; 2]) -> Screen {
        Screen {
            title: String::from("spotty"),

            grid: crate::grid::CharacterGrid::new(grid_size[0], grid_size[1]),
            alternate_grid: crate::grid::CharacterGrid::new(grid_size[0], grid_size[1]),

            cursor: crate::grid::Position::new(0, 0),
            saved_cursor: crate::grid::Position::new(0, 0),
            cursor_style: crate::tty::control_code::CursorStyle::DEFAULT,
            cursor_color: crate::color::DEFAULT_CURSOR,

            style: crate::tty::control_code::CharacterStyles::empty(),
            foreground: crate::color::DEFAULT_FOREGROUND,

            background: crate::color::DEFAULT_BACKGROUND,

            scrolling_region: 0..grid_size[0],
            behaviours: Behaviours::default(),
            residual_input: Vec::new(),
        }
    }

    pub fn resize_grid(&mut self, grid_size: [u16; 2]) {
        self.grid = crate::grid::CharacterGrid::new(grid_size[0], grid_size[1]);
        self.alternate_grid = crate::grid::CharacterGrid::new(grid_size[0], grid_size[1]);

        self.cursor = crate::grid::Position::new(0, 0);

        self.scrolling_region = 0..grid_size[0];
    }

    pub fn process_input(&mut self, input: &[u8]) {
        let mut bytes;

        let bytes = if self.residual_input.is_empty() {
            input
        } else {
            bytes = Vec::with_capacity(self.residual_input.len() + input.len());
            bytes.append(&mut self.residual_input);
            bytes.extend_from_slice(input);
            &bytes
        };

        let residual = crate::tty::control_code::parse(bytes, self);
        self.residual_input.extend_from_slice(residual);
    }

    pub fn cursor_render_state(
        &self,
        palette: &crate::color::Palette,
    ) -> Option<crate::render::CursorState> {
        if self.behaviours.show_cursor {
            Some(crate::render::CursorState {
                position: self.virtual_cursor(),
                style: self.cursor_style,
                color: self.cursor_color,
                text_color: self.cursor_color.complement(palette),
            })
        } else {
            None
        }
    }

    fn virtual_cursor(&self) -> crate::grid::Position {
        let mut position = self.cursor;

        if position.col >= self.grid.cols() {
            position.col = 0;
            position.row += 1;
        }
        if position.row >= self.grid.rows() {
            position.row = self.grid.max_row();
            position.col = self.grid.max_col();
        }

        position
    }
}

#[allow(unused_variables)]
impl crate::tty::control_code::Terminal for Screen {
    fn invalid_control_sequence(&mut self, bytes: &[u8]) {
        let text = String::from_utf8_lossy(bytes);
        warn!(?text, "invalid control sequence");
    }

    fn text(&mut self, text: &str) {
        debug!(?text);

        for ch in text.chars() {
            self.insert_char(ch);
        }
    }

    fn invalid_utf8(&mut self, text: &[u8]) {
        warn!(?text, "invalid_utf8");

        let mut buffer = [0u8; 4];
        self.text(char::REPLACEMENT_CHARACTER.encode_utf8(&mut buffer));
    }

    fn bell(&mut self) {
        trace!("bell");
    }

    fn tab(&mut self) {
        trace!("tab");

        loop {
            self.advance_column();
            if self.cursor.col % 8 == 0 {
                break;
            }
        }
    }

    fn backspace(&mut self) {
        debug!("backspace");

        if self.cursor.col > 0 {
            self.cursor.col -= 1;
        } else {
            self.cursor.col = self.grid.cols() - 1;
        }
    }

    fn carriage_return(&mut self) {
        debug!("carriage_return");

        self.cursor.col = 0
    }

    fn line_feed(&mut self) {
        debug!("line_feed");

        self.advance_row();
    }

    fn reverse_line_feed(&mut self) {
        trace!("reverse_line_feed");

        self.cursor.col = 0;
        if self.cursor.row > self.scrolling_region.start {
            self.cursor.row -= 1;
        } else {
            self.scroll_down(1);
        }
    }

    fn delete_lines(&mut self, count: u16) {
        debug!(?count, "delete_lines");

        let clear_end = self
            .cursor
            .row
            .saturating_add(count)
            .min(self.scrolling_region.end);

        self.grid
            .copy_rows(clear_end..self.scrolling_region.end, self.cursor.row);

        let rows_below = self.scrolling_region.end - clear_end;
        let copy_end = self.cursor.row + rows_below;
        self.clear_region(copy_end..self.scrolling_region.end, ..)
    }

    fn insert_lines(&mut self, count: u16) {
        debug!(?count, "insert_lines");

        let clear_end = self
            .cursor
            .row
            .saturating_add(count)
            .min(self.scrolling_region.end);

        let rows_below = self.scrolling_region.end - clear_end;
        let copy_end = self.cursor.row + rows_below;
        self.grid.copy_rows(self.cursor.row..copy_end, clear_end);

        self.clear_region(self.cursor.row..clear_end, ..);
    }

    fn scroll_down(&mut self, count: u16) {
        debug!(?count, "scroll_down");

        let copy_destination = count;
        let copy_start = self.scrolling_region.start;
        let copy_end = self.scrolling_region.end.saturating_sub(count);

        if copy_start > copy_end {
            return;
        }

        self.grid.copy_rows(copy_start..copy_end, copy_destination);

        let clear_start = self.scrolling_region.start;
        let clear_end = count;
        self.clear_region(clear_start..clear_end, ..);
    }

    fn scroll_up(&mut self, count: u16) {
        debug!(?count, "scroll_up");

        let copy_destination = self.scrolling_region.start;
        let copy_start = self.scrolling_region.start + count;
        let copy_end = self.scrolling_region.end;

        if copy_start > copy_end {
            return;
        }

        self.grid.copy_rows(copy_start..copy_end, copy_destination);

        let clear_start = self.scrolling_region.end - count;
        let clear_end = self.scrolling_region.end;
        self.clear_region(clear_start..clear_end, ..);
    }

    fn move_cursor(&mut self, direction: crate::tty::control_code::Direction, steps: u16) {
        debug!(?direction, ?steps, "move_cursor");

        use crate::tty::control_code::Direction;

        match direction {
            Direction::Up => self.cursor.row = self.cursor.row.saturating_sub(steps),
            Direction::Down => {
                self.cursor.row = self
                    .cursor
                    .row
                    .saturating_add(steps)
                    .min(self.grid.max_row())
            }
            Direction::Left => self.cursor.col = self.cursor.col.saturating_sub(steps),
            Direction::Right => {
                self.cursor.col = self
                    .cursor
                    .col
                    .saturating_add(steps)
                    .min(self.grid.max_col())
            }
        }
    }

    fn set_cursor_pos(&mut self, row: u16, col: u16) {
        self.set_cursor_row(row);
        self.set_cursor_col(col);
    }

    fn set_cursor_row(&mut self, row: u16) {
        debug!(?row, "set_cursor_row");
        self.cursor.row = row.min(self.grid.max_row());
    }

    fn set_cursor_col(&mut self, col: u16) {
        debug!(?col, "set_cursor_col");
        self.cursor.col = col.min(self.grid.max_col());
    }

    fn save_cursor(&mut self) {
        debug!(?self.cursor, "save_cursor");
        self.saved_cursor = self.cursor;
    }

    fn restore_cursor(&mut self) {
        debug!(?self.saved_cursor, "restore_cursor");
        self.cursor.row = self.saved_cursor.row.min(self.grid.max_row());
        self.cursor.col = self.saved_cursor.col.min(self.grid.max_col());
    }

    fn set_cursor_style(&mut self, style: crate::tty::control_code::CursorStyle) {
        debug!(?style, "set_cursor_style");
        self.cursor_style = style;
    }

    fn set_cursor_color(&mut self, color: crate::color::Color) {
        self.cursor_color = color;
    }

    fn reset_cursor_color(&mut self) {
        debug!("reset_cursor_color");
        self.cursor_color = crate::color::DEFAULT_CURSOR;
    }

    fn set_scrolling_region(&mut self, rows: std::ops::Range<u16>) {
        debug!(?rows, "set_scrolling_region");

        self.scrolling_region.start = rows.start.min(self.grid.rows());
        self.scrolling_region.end = rows.end.min(self.grid.rows());
    }

    fn clear_line(&mut self, region: crate::tty::control_code::ClearRegion) {
        debug!(?region, "clear_line");

        match region {
            crate::tty::control_code::ClearRegion::ToEnd => {
                self.clear_current_line(self.cursor.col..)
            }
            crate::tty::control_code::ClearRegion::ToStart => {
                self.clear_current_line(..=self.cursor.col)
            }
            crate::tty::control_code::ClearRegion::All => self.clear_current_line(..),
        }
    }

    fn clear_screen(&mut self, region: crate::tty::control_code::ClearRegion) {
        debug!(?region, "clear_screen");

        match region {
            crate::tty::control_code::ClearRegion::ToEnd => {
                self.clear_current_line(self.cursor.col..);
                self.clear_region(self.cursor.row + 1.., ..);
            }
            crate::tty::control_code::ClearRegion::ToStart => {
                self.clear_region(..self.cursor.row, ..);
                self.clear_current_line(..=self.cursor.col);
            }
            crate::tty::control_code::ClearRegion::All => self.clear_region(.., ..),
        }
    }

    fn clear_scrollback(&mut self) {
        todo!("buffer command: clear_scrollback")
    }

    fn erase(&mut self, count: u16) {
        debug!(?count, "erase characters");
        self.clear_current_line(self.cursor.col..self.cursor.col.saturating_add(count));
    }

    fn set_character_style(&mut self, style: crate::tty::control_code::CharacterStyles) {
        debug!(?style, "set_character_style");
        self.style.insert(style);
    }

    fn reset_character_style(&mut self, style: crate::tty::control_code::CharacterStyles) {
        debug!(?style, "reset_character_style");
        self.style.remove(style);
    }

    fn set_foreground_color(&mut self, color: crate::color::Color) {
        debug!(?color, "set_foreground_color");
        self.foreground = color;
    }

    fn reset_foreground_color(&mut self) {
        debug!("reset_foreground_color");
        self.foreground = crate::color::DEFAULT_FOREGROUND;
    }

    fn set_background_color(&mut self, color: crate::color::Color) {
        debug!(?color, "set_background_color");
        self.background = color;
    }

    fn reset_background_color(&mut self) {
        debug!("reset_background_color");
        self.background = crate::color::DEFAULT_BACKGROUND;
    }

    fn set_window_title(&mut self, text: &str) {
        debug!(?text, "set_window_title");
        self.title = text.to_owned();
    }

    fn toggle_behaviour(
        &mut self,
        behaviour: crate::tty::control_code::Behaviour,
        toggle: crate::tty::control_code::Toggle,
    ) {
        debug!(?behaviour, ?toggle, "toggle_behaviour");

        use crate::tty::control_code::Behaviour;

        match behaviour {
            Behaviour::ShowCursor => self.behaviours.show_cursor = toggle.is_enabled(),
            Behaviour::AlternateBuffer => {
                if toggle.is_enabled() != self.behaviours.alternate_buffer {
                    self.behaviours.alternate_buffer = toggle.is_enabled();
                    std::mem::swap(&mut self.grid, &mut self.alternate_grid);
                }
            }
            Behaviour::BracketedPaste => self.behaviours.bracketed_paste = toggle.is_enabled(),
            _ => warn!(?behaviour, ?toggle, "unimplemented behaviour"),
        }
    }
}

impl Screen {
    fn advance_column(&mut self) {
        if self.cursor.col < self.grid.cols() {
            self.cursor.col += 1;
        } else {
            self.cursor.col = 0;
            self.advance_row();
        }
    }

    fn advance_row(&mut self) {
        if self.cursor.row < self.scrolling_region.end.saturating_sub(1) {
            self.cursor.row += 1;
        } else {
            use crate::tty::control_code::Terminal;
            self.scroll_up(1);
        }
    }

    fn insert_char(&mut self, ch: char) {
        if self.cursor.col == self.grid.cols() {
            self.cursor.col = 0;
            self.advance_row();
        }

        self.grid[self.cursor] = crate::grid::GridCell {
            character: ch,
            foreground: self.foreground,
            background: self.background,
            style: self.style,
        };
        self.advance_column();
    }

    fn clear_current_line(&mut self, columns: impl std::ops::RangeBounds<u16>) {
        self.clear_region(self.cursor.row..=self.cursor.row, columns)
    }

    fn clear_region(
        &mut self,
        rows: impl std::ops::RangeBounds<u16>,
        columns: impl std::ops::RangeBounds<u16>,
    ) {
        let cell = self.empty_cell();
        self.grid.fill_region(rows, columns, cell);
    }

    fn empty_cell(&self) -> crate::grid::GridCell {
        crate::grid::GridCell {
            character: ' ',
            foreground: crate::color::DEFAULT_FOREGROUND,
            background: crate::color::DEFAULT_BACKGROUND,
            style: self.style,
        }
    }
}
