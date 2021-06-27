#[cfg_attr(target_os = "macos", path = "font/core_text.rs")]
mod platform;

pub use platform::Font;

use std::sync::Arc;

#[derive(Clone)]
pub struct FontCollection {
    pub regular: Arc<crate::font::Font>,
    pub bold: Arc<crate::font::Font>,
    pub italic: Arc<crate::font::Font>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Style {
    Regular,
    Bold,
    Italic,
}

impl FontCollection {
    pub fn get_with_style(&self, style: Style) -> Arc<Font> {
        match style {
            Style::Regular => self.regular.clone(),
            Style::Bold => self.bold.clone(),
            Style::Italic => self.italic.clone(),
        }
    }
}

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

pub fn cell_size(font: &crate::font::Font) -> [f32; 2] {
    let metrics = font.metrics();
    [metrics.advance, metrics.line_height]
}
