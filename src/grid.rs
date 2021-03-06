pub struct CharacterGrid {
    rows: u16,
    cols: u16,
    cells: Vec<GridCell>,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct Position {
    pub row: u16,
    pub col: u16,
}

impl Position {
    pub const fn new(row: u16, col: u16) -> Position {
        Position { row, col }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct GridCell {
    pub character: char,
    pub foreground: crate::color::Color,
    pub background: crate::color::Color,
    pub style: crate::tty::control_code::CharacterStyles,
}

impl GridCell {
    pub const fn empty() -> Self {
        GridCell {
            character: ' ',
            foreground: crate::color::DEFAULT_FOREGROUND,
            background: crate::color::DEFAULT_BACKGROUND,
            style: crate::tty::control_code::CharacterStyles::empty(),
        }
    }
}

impl Default for GridCell {
    fn default() -> Self {
        GridCell::empty()
    }
}

pub fn size_in_window(window_size: crate::window::PhysicalSize, cell_size: [f32; 2]) -> [u16; 2] {
    let cols = window_size.width as f32 / cell_size[0];
    let rows = window_size.height as f32 / cell_size[1];

    let cols = f32::clamp(cols.floor(), 1.0, f32::from(u16::MAX));
    let rows = f32::clamp(rows.floor(), 1.0, f32::from(u16::MAX));

    // SAFETY: cols and rows are in the representable range of a u16
    let cols = unsafe { cols.to_int_unchecked::<u16>() };
    let rows = unsafe { rows.to_int_unchecked::<u16>() };

    [rows, cols]
}

impl CharacterGrid {
    pub fn new(rows: u16, cols: u16) -> CharacterGrid {
        CharacterGrid {
            rows,
            cols,
            cells: vec![GridCell::default(); cols as usize * rows as usize],
        }
    }

    pub fn size(&self) -> [u16; 2] {
        [self.rows, self.cols]
    }

    pub fn cols(&self) -> u16 {
        self.cols
    }

    pub fn rows(&self) -> u16 {
        self.rows
    }

    pub fn max_col(&self) -> u16 {
        self.cols - 1
    }

    pub fn max_row(&self) -> u16 {
        self.rows - 1
    }

    pub fn fill_region(
        &mut self,
        row_range: impl std::ops::RangeBounds<u16>,
        col_range: impl std::ops::RangeBounds<u16>,
        cell: GridCell,
    ) {
        let rows = into_exclusive_range(row_range, self.rows);
        let columns = into_exclusive_range(col_range, self.cols);

        // skip iterating over every row every column is cleared
        if columns.start == 0 && columns.end == self.max_col() {
            let row_start = rows.start as usize * self.cols as usize;
            let row_end = rows.end as usize * self.cols as usize;
            self.cells[row_start..row_end].fill(cell);
        } else {
            for row in rows {
                let row_index = row as usize * self.cols as usize;

                let row_start = columns.start as usize + row_index;
                let row_end = columns.end as usize + row_index;

                self.cells[row_start..row_end].fill(cell);
            }
        }
    }

    pub fn copy_rows(&mut self, rows: impl std::ops::RangeBounds<u16>, dst_row: u16) {
        let rows = into_exclusive_range(rows, self.rows);

        let row_start = rows.start as usize * self.cols as usize;
        let row_end = rows.end as usize * self.cols as usize;

        let dst_start = dst_row as usize * self.cols as usize;

        self.cells.copy_within(row_start..row_end, dst_start);
    }
}

impl std::ops::Index<Position> for CharacterGrid {
    type Output = GridCell;

    fn index(&self, pos: Position) -> &Self::Output {
        assert!(
            pos.col < self.cols && pos.row < self.rows,
            "out of grid bounds (index = [{}, {}], size = [{}, {}])",
            pos.row,
            pos.col,
            self.rows,
            self.cols
        );
        &self.cells[pos.col as usize + pos.row as usize * self.cols as usize]
    }
}

impl std::ops::IndexMut<Position> for CharacterGrid {
    fn index_mut(&mut self, pos: Position) -> &mut Self::Output {
        assert!(
            pos.col < self.cols && pos.row < self.rows,
            "out of grid bounds (index = [{}, {}], size = [{}, {}])",
            pos.row,
            pos.col,
            self.rows,
            self.cols
        );
        &mut self.cells[pos.col as usize + pos.row as usize * self.cols as usize]
    }
}

impl std::fmt::Debug for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[")?;
        self.row.fmt(f)?;
        write!(f, ", ")?;
        self.col.fmt(f)?;
        write!(f, "]")?;
        Ok(())
    }
}

fn into_exclusive_range(range: impl std::ops::RangeBounds<u16>, max: u16) -> std::ops::Range<u16> {
    let start = match range.start_bound() {
        std::ops::Bound::Included(index) => *index,
        std::ops::Bound::Excluded(index) => *index + 1,
        std::ops::Bound::Unbounded => 0,
    };

    let end = match range.end_bound() {
        std::ops::Bound::Included(index) => *index + 1,
        std::ops::Bound::Excluded(index) => *index,
        std::ops::Bound::Unbounded => max,
    };

    start.min(max)..end.min(max)
}
