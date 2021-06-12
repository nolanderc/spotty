
#[cfg_attr(target_os = "macos", path = "font/core_text.rs")]
mod platform;

pub use platform::Font;

#[derive(Debug, Copy, Clone)]
pub struct FontMetrics {
    pub advance: f32,
    pub line_height: f32,
    pub ascent: f32,
    pub descent: f32,
}

pub struct RasterizedGlyph {
    pub bitmap: Bitmap,
    pub metrics: GlyphMetrics,
}

#[derive(Debug, Copy, Clone)]
pub struct GlyphMetrics {
    pub ascent: i32,
    pub bearing: i32,
}

pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[u8; 4]>,
}

