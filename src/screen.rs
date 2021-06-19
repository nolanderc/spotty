pub struct Screen {
    pub title: String,

    pub grid: crate::grid::CharacterGrid,
    pub cursor: crate::grid::Position,
    pub styles: crate::tty::control_code::CharacterStyles,

    pub foreground: crate::color::Color,
    pub background: crate::color::Color,

    /// Output from the shell that hasn't been parsed yet due to needing more bytes.
    residual_input: Vec<u8>,
}

impl Screen {
    pub fn new(grid_size: [u16; 2]) -> Screen {
        let grid = crate::grid::CharacterGrid::new(grid_size[0], grid_size[1]);
        let cursor = crate::grid::Position::new(0, 0);

        Screen {
            title: String::from("spotty"),

            grid,
            cursor,
            styles: crate::tty::control_code::CharacterStyles::empty(),

            foreground: crate::color::DEFAULT_FOREGROUND,
            background: crate::color::DEFAULT_BACKGROUND,

            residual_input: Vec::new(),
        }
    }

    pub fn process_input(&mut self, input: &[u8]) {
        let mut bytes;

        let residual = if self.residual_input.is_empty() {
            crate::tty::control_code::parse(input, self)
        } else {
            bytes = Vec::with_capacity(self.residual_input.len() + input.len());
            bytes.append(&mut self.residual_input);
            bytes.extend_from_slice(input);

            crate::tty::control_code::parse(&bytes, self)
        };

        self.residual_input.extend_from_slice(residual);
    }

    pub fn advance_column(&mut self) {
        if self.cursor.col < self.grid.max_col() {
            self.cursor.col += 1;
        } else {
            self.cursor.col = 0;
            self.advance_row();
        }
    }

    pub fn advance_row(&mut self) {
        if self.cursor.row < self.grid.max_row() {
            self.cursor.row += 1;
        } else {
            self.grid.scroll_up(1);
        }
    }

    pub fn clear_current_line(&mut self, columns: impl std::ops::RangeBounds<u16>) {
        self.grid
            .clear_region(self.cursor.row..=self.cursor.row, columns)
    }

    fn get_color(&self, color: crate::tty::control_code::Color) -> crate::color::Color {
        match color {
            crate::tty::control_code::Color::Index(index) => {
                crate::color::DEFAULT_PALETTE[index as usize]
            }
            crate::tty::control_code::Color::Rgb(rgb) => rgb.into(),
        }
    }
}

#[allow(unused_variables)]
impl crate::tty::control_code::Terminal for Screen {
    fn invalid_control_sequence(&mut self, bytes: &[u8]) {
        let text = String::from_utf8_lossy(bytes);
        eprintln!("invalid control sequence: {:?}", text);
    }

    fn text(&mut self, text: &str) {
        for ch in text.chars() {
            self.grid[self.cursor] = crate::grid::GridCell {
                character: ch,
                foreground: self.foreground,
                background: self.background,
            };
            self.advance_column();
        }
    }

    fn invalid_utf8(&mut self, text: &[u8]) {
        todo!("buffer command: invalid_utf8")
    }

    fn bell(&mut self) {
        eprintln!("Bell!!!");
    }

    fn tab(&mut self) {
        loop {
            self.advance_column();
            if self.cursor.col % 8 == 0 {
                break;
            }
        }
    }

    fn backspace(&mut self) {
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
        } else {
            self.cursor.col = self.grid.cols() - 1;
        }
    }

    fn carriage_return(&mut self) {
        self.cursor.col = 0
    }

    fn line_feed(&mut self) {
        self.cursor.col = 0;
        self.advance_row();
    }

    fn reverse_line_feed(&mut self) {
        self.cursor.col = 0;
        if self.cursor.row > 0 {
            self.cursor.row += 1;
        } else {
            self.grid.scroll_down(1);
        }
    }


    fn move_cursor(&mut self, direction: crate::tty::control_code::Direction, steps: u16) {
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
        self.cursor.row = row.min(self.grid.max_row());
        self.cursor.col = col.min(self.grid.max_col());
    }

    fn clear_line(&mut self, region: crate::tty::control_code::ClearRegion) {
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
        match region {
            crate::tty::control_code::ClearRegion::ToEnd => {
                self.clear_current_line(self.cursor.col..);
                self.grid.clear_region(self.cursor.row + 1.., ..);
            }
            crate::tty::control_code::ClearRegion::ToStart => {
                self.grid.clear_region(..self.cursor.row, ..);
                self.clear_current_line(..=self.cursor.col);
            }
            crate::tty::control_code::ClearRegion::All => self.grid.clear_region(.., ..),
        }
    }

    fn clear_scrollback(&mut self) {
        todo!("buffer command: clear_scrollback")
    }

    fn set_bracketed_paste(&mut self, toggle: crate::tty::control_code::Toggle) {
        eprintln!("TODO: buffer command: set_bracketed_paste")
    }

    fn set_application_cursor(&mut self, toggle: crate::tty::control_code::Toggle) {
        eprintln!("TODO: buffer command: set_application_cursor")
    }

    fn set_character_style(&mut self, style: crate::tty::control_code::CharacterStyles) {
        self.styles.insert(style);
    }

    fn reset_character_style(&mut self, style: crate::tty::control_code::CharacterStyles) {
        self.styles.remove(style);
    }

    fn set_foreground_color(&mut self, color: crate::tty::control_code::Color) {
        self.foreground = self.get_color(color);
    }

    fn reset_foreground_color(&mut self) {
        self.foreground = crate::color::DEFAULT_FOREGROUND;
    }

    fn set_background_color(&mut self, color: crate::tty::control_code::Color) {
        self.background = self.get_color(color);
    }

    fn reset_background_color(&mut self) {
        self.background = crate::color::DEFAULT_BACKGROUND;
    }

    fn set_window_title(&mut self, text: &str) {
        self.title = text.to_owned();
    }
}
