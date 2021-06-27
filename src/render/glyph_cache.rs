use std::collections::HashMap;

pub struct GlyphCache {
    font: crate::font::FontCollection,
    atlas: super::texture_atlas::TextureAtlas,
    glyphs: HashMap<(char, crate::font::Style), Glyph>,
}

#[derive(Debug, Copy, Clone)]
pub struct Glyph {
    pub offset: [u16; 2],
    pub size: [u16; 2],
    pub metrics: crate::font::GlyphMetrics,
}

#[derive(Debug, Copy, Clone)]
pub enum RasterizationError {
    MissingGlyph,
    AtlasFull,
}

impl GlyphCache {
    pub fn new(font: crate::font::FontCollection, atlas_size: usize) -> GlyphCache {
        GlyphCache {
            font,
            atlas: super::texture_atlas::TextureAtlas::new(atlas_size),
            glyphs: HashMap::new(),
        }
    }

    pub fn font(&self) -> &crate::font::FontCollection {
        &self.font
    }

    pub fn get(&self, ch: char, style: crate::font::Style) -> Option<Glyph> {
        self.glyphs.get(&(ch, style)).copied()
    }

    pub fn rasterize(
        &mut self,
        ch: char,
        style: crate::font::Style,
    ) -> Result<(Glyph, Vec<[u8; 4]>), RasterizationError> {
        let font = self.font.get_with_style(style);

        let rasterized = font.rasterize(ch).ok_or(RasterizationError::MissingGlyph)?;

        let offset = self
            .atlas
            .reserve(
                rasterized.bitmap.width as usize,
                rasterized.bitmap.height as usize,
            )
            .ok_or(RasterizationError::AtlasFull)?;

        let glyph = Glyph {
            offset,
            size: [
                rasterized.bitmap.width as u16,
                rasterized.bitmap.height as u16,
            ],
            metrics: rasterized.metrics,
        };

        self.glyphs.insert((ch, style), glyph);

        Ok((glyph, rasterized.bitmap.pixels))
    }
}
