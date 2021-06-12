mod glyph_cache;
#[cfg(target_os = "macos")]
mod metal;
mod texture_atlas;

#[cfg(target_os = "macos")]
use self::metal as platform;

pub use platform::Renderer;

const FONT_ATLAS_SIZE: usize = 512;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    position: [f32; 2],
    tex_coord: [f32; 2],
    color: [f32; 4],
}

impl Vertex {
    pub fn new(position: [f32; 2], tex_coord: [f32; 2], color: [f32; 4]) -> Vertex {
        Vertex {
            position,
            tex_coord,
            color,
        }
    }

    fn glyph_quad(glyph: glyph_cache::Glyph, position: [f32; 2], color: [f32; 4]) -> [Vertex; 6] {
        let width = glyph.size[0] as f32;
        let height = glyph.size[1] as f32;

        let pos_x = position[0] + glyph.metrics.bearing as f32;
        let pos_y = position[1] - glyph.metrics.ascent as f32;

        let pos_l = pos_x;
        let pos_r = pos_x + width;
        let pos_t = pos_y;
        let pos_b = pos_y + height;

        let tex_x = glyph.offset[0] as f32 / FONT_ATLAS_SIZE as f32;
        let tex_y = glyph.offset[1] as f32 / FONT_ATLAS_SIZE as f32;
        let tex_width = glyph.size[0] as f32 / FONT_ATLAS_SIZE as f32;
        let tex_height = glyph.size[1] as f32 / FONT_ATLAS_SIZE as f32;

        let tex_l = tex_x;
        let tex_r = tex_x + tex_width;
        let tex_t = tex_y;
        let tex_b = tex_y + tex_height;

        Vertex::quad(
            [pos_l, pos_r, pos_t, pos_b],
            [tex_l, tex_r, tex_t, tex_b],
            color,
        )
    }

    pub fn quad(pos_quad: [f32; 4], tex_quad: [f32; 4], color: [f32; 4]) -> [Vertex; 6] {
        let [pos_l, pos_r, pos_t, pos_b] = pos_quad;
        let [tex_l, tex_r, tex_t, tex_b] = tex_quad;

        [
            Vertex::new([pos_l, pos_t], [tex_l, tex_t], color),
            Vertex::new([pos_l, pos_b], [tex_l, tex_b], color),
            Vertex::new([pos_r, pos_b], [tex_r, tex_b], color),
            Vertex::new([pos_r, pos_b], [tex_r, tex_b], color),
            Vertex::new([pos_r, pos_t], [tex_r, tex_t], color),
            Vertex::new([pos_l, pos_t], [tex_l, tex_t], color),
        ]
    }
}

pub struct CharacterGrid {
    width: u16,
    height: u16,
    cells: Vec<GridCell>,
}

#[derive(Debug, Copy, Clone)]
pub struct GridCell {
    pub character: char,
}

impl Default for GridCell {
    fn default() -> Self {
        GridCell { character: ' ' }
    }
}

impl CharacterGrid {
    pub fn new(width: u16, height: u16) -> CharacterGrid {
        CharacterGrid {
            width,
            height,
            cells: vec![GridCell::default(); width as usize * height as usize],
        }
    }

    pub fn in_window(
        window_size: crate::window::PhysicalSize,
        cell_size: [f32; 2],
    ) -> CharacterGrid {
        let cells_x = window_size.width as f32 / cell_size[0];
        let cells_y = window_size.height as f32 / cell_size[1];

        let width = f32::clamp(cells_x.floor(), 1.0, f32::from(u16::MAX));
        let height = f32::clamp(cells_y.floor(), 1.0, f32::from(u16::MAX));

        // SAFETY: width and height are in the representable range of a u16
        let width = unsafe { width.to_int_unchecked::<u16>() };
        let height = unsafe { height.to_int_unchecked::<u16>() };

        CharacterGrid::new(width, height)
    }

    pub fn width(&self) -> u16 {
        self.width
    }

    pub fn height(&self) -> u16 {
        self.height
    }

    pub fn get(&self, x: u16, y: u16) -> Option<&GridCell> {
        if x < self.width && y < self.height {
            Some(&self[[x as u16, y as u16]])
        } else {
            None
        }
    }

    pub fn get_mut(&mut self, x: u16, y: u16) -> Option<&mut GridCell> {
        if x < self.width && y < self.height {
            Some(&mut self[[x as u16, y as u16]])
        } else {
            None
        }
    }
}

impl std::ops::Index<[u16; 2]> for CharacterGrid {
    type Output = GridCell;

    fn index(&self, [x, y]: [u16; 2]) -> &Self::Output {
        assert!(
            x < self.width && y < self.height,
            "out of grid bounds (index = [{}, {}], size = [{}, {}])",
            x,
            y,
            self.width,
            self.height
        );
        &self.cells[x as usize + y as usize * self.width as usize]
    }
}

impl std::ops::IndexMut<[u16; 2]> for CharacterGrid {
    fn index_mut(&mut self, [x, y]: [u16; 2]) -> &mut Self::Output {
        assert!(
            x < self.width && y < self.height,
            "out of grid bounds (index = [{}, {}], size = [{}, {}])",
            x,
            y,
            self.width,
            self.height
        );
        &mut self.cells[x as usize + y as usize * self.width as usize]
    }
}
