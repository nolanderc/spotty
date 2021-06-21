mod glyph_cache;
#[cfg(target_os = "macos")]
mod metal;
mod texture_atlas;

#[cfg(target_os = "macos")]
use self::metal as platform;

pub use platform::Renderer;

const FONT_ATLAS_SIZE: usize = 2048;

pub struct CursorState {
    pub position: crate::grid::Position,
    pub style: crate::tty::control_code::CursorStyle,
    pub color: crate::color::Color,
    pub text_color: crate::color::Color,
}

impl CursorState {
    pub const fn invisible() -> CursorState {
        CursorState {
            position: crate::grid::Position::new(u16::MAX, u16::MAX),
            style: crate::tty::control_code::CursorStyle::DEFAULT,
            color: crate::color::Color::BLACK,
            text_color: crate::color::Color::BLACK,
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
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
