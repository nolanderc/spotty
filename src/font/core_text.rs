use std::sync::Arc;

pub struct Font {
    cascade: Vec<core_text::font::CTFont>,
    metrics: super::FontMetrics,
}

impl Font {
    pub fn collection(name: &str, pt_size: f64) -> Option<super::FontCollection> {
        let family = core_text::font_collection::create_for_family(name)?;
        let descriptors = family.get_descriptors()?;
        let fonts = descriptors
            .into_iter()
            .map(|desc| core_text::font::new_from_descriptor(&desc, pt_size));

        let mut regular = None;
        let mut bold = None;
        let mut italic = None;

        for font in fonts {
            use core_text::font_descriptor::SymbolicTraitAccessors;

            let traits = font.symbolic_traits();
            if traits.is_expanded()
                || traits.is_condensed()
                || traits.is_vertical()
                || !traits.is_monospace()
            {
                continue;
            }

            let mut style_name = font.style_name();
            style_name.make_ascii_lowercase();
            match style_name.as_str().trim() {
                "regular" => regular = Some(Self::from_ct_font(font)),
                "bold" => bold = Some(Self::from_ct_font(font)),
                "italic" => italic = Some(Self::from_ct_font(font)),

                _ => match (traits.is_bold(), traits.is_italic()) {
                    (false, false) if regular.is_none() => regular = Some(Self::from_ct_font(font)),
                    (true, false) if bold.is_none() => bold = Some(Self::from_ct_font(font)),
                    (false, true) if italic.is_none() => italic = Some(Self::from_ct_font(font)),
                    _ => {}
                },
            }
        }

        let regular = regular.map(Arc::new)?;
        let bold = bold.map(Arc::new).unwrap_or_else(|| Arc::clone(&regular));
        let italic = italic.map(Arc::new).unwrap_or_else(|| Arc::clone(&regular));

        core_text::font_descriptor::debug_descriptor(&regular.cascade[0].copy_descriptor());
        core_text::font_descriptor::debug_descriptor(&bold.cascade[0].copy_descriptor());
        core_text::font_descriptor::debug_descriptor(&italic.cascade[0].copy_descriptor());

        Some(super::FontCollection {
            regular,
            bold,
            italic,
        })
    }

    fn from_ct_font(font: core_text::font::CTFont) -> Font {
        let metrics = Self::extract_metrics(&font);

        let languages = core_foundation::array::CFArray::from_CFTypes(&[
            core_foundation::string::CFString::new("en"),
        ]);
        let fallbacks = core_text::font::cascade_list_for_languages(&font, &languages);
        let fallback_fonts = fallbacks
            .into_iter()
            .map(|descriptor| core_text::font::new_from_descriptor(&descriptor, font.pt_size()));

        let mut cascade = Vec::with_capacity(1 + fallbacks.len() as usize);
        cascade.extend(fallback_fonts);
        cascade.insert(0, font);

        Font { cascade, metrics }
    }

    fn extract_metrics(font: &core_text::font::CTFont) -> super::FontMetrics {
        let dummy_glyph = unsafe {
            let mut glyphs = [0; 2];
            font.get_glyphs_for_characters(&(b'a' as u16) as *const _, glyphs.as_mut_ptr(), 1);
            glyphs[0]
        };

        let advance = unsafe {
            font.get_advances_for_glyphs(
                core_text::font_descriptor::kCTFontHorizontalOrientation,
                &dummy_glyph as *const _,
                std::ptr::null_mut(),
                1,
            )
        };

        let ascent = font.ascent();
        let descent = font.descent();
        let line_height = ascent + descent + font.leading();

        super::FontMetrics {
            advance: advance as f32,
            line_height: line_height as f32,
            ascent: ascent as f32,
            descent: descent as f32,
        }
    }

    pub fn metrics(&self) -> &super::FontMetrics {
        &self.metrics
    }

    pub fn rasterize(&self, ch: char) -> Option<super::RasterizedGlyph> {
        let (glyph, font) = find_glyph(ch, &self.cascade)?;

        let bounds = font.get_bounding_rects_for_glyphs(
            core_text::font_descriptor::kCTFontHorizontalOrientation,
            &[glyph],
        );

        let raster_left = bounds.origin.x.floor() as i32;
        let raster_width =
            (bounds.size.width + bounds.origin.x - raster_left as f64).ceil() as usize;

        let raster_descent = (-bounds.origin.y).ceil() as i32;
        let raster_ascent = (bounds.size.height + bounds.origin.y).ceil() as i32;
        let raster_height = 1 + (raster_ascent + raster_descent) as usize;

        let metrics = super::GlyphMetrics {
            ascent: raster_ascent,
            bearing: raster_left,
        };

        let mut bitmap = super::Bitmap {
            width: raster_width as u32,
            height: raster_height as u32,
            pixels: vec![[0u8; 4]; raster_width * raster_height],
        };

        if raster_width > 0 && raster_height > 0 {
            let color_space = {
                let name = unsafe { core_graphics::color_space::kCGColorSpaceSRGB };
                core_graphics::color_space::CGColorSpace::create_with_name(name)
                    .unwrap_or_else(core_graphics::color_space::CGColorSpace::create_device_rgb)
            };

            let draw_context = core_graphics::context::CGContext::create_bitmap_context(
                Some(bitmap.pixels.as_mut_ptr() as *mut _),
                raster_width,
                raster_height,
                8,
                raster_width * 4,
                &color_space,
                core_graphics::base::kCGImageAlphaPremultipliedLast
                    | core_graphics::base::kCGBitmapByteOrder32Big,
            );

            draw_context.set_allows_antialiasing(true);
            draw_context.set_allows_font_smoothing(true);
            draw_context.set_allows_font_subpixel_positioning(true);
            draw_context.set_allows_font_subpixel_quantization(true);

            draw_context.set_should_antialias(true);
            draw_context.set_should_smooth_fonts(true);
            draw_context.set_should_subpixel_position_fonts(true);
            draw_context.set_should_subpixel_quantize_fonts(true);

            draw_context.set_rgb_fill_color(1.0, 1.0, 1.0, 1.0);
            font.draw_glyphs(
                &[glyph],
                &[core_graphics::geometry::CGPoint::new(
                    -raster_left as f64,
                    raster_descent as f64,
                )],
                draw_context.clone(),
            );
            draw_context.flush();
            drop(draw_context);
        }

        Some(super::RasterizedGlyph { bitmap, metrics })
    }
}

fn find_glyph(
    ch: char,
    cascade: &[core_text::font::CTFont],
) -> Option<(core_graphics::base::CGGlyph, &core_text::font::CTFont)> {
    cascade
        .iter()
        .find_map(|font| glyph_index(font, ch).map(|glyph| (glyph, font)))
}

fn glyph_index(font: &core_text::font::CTFont, ch: char) -> Option<core_graphics::base::CGGlyph> {
    let mut buffer = [0u16; 2];
    let encoded = ch.encode_utf16(&mut buffer);

    unsafe {
        let mut glyphs = [0; 2];
        let success = font.get_glyphs_for_characters(
            encoded.as_ptr(),
            glyphs.as_mut_ptr(),
            encoded.len() as isize,
        );

        if success {
            Some(glyphs[0])
        } else {
            None
        }
    }
}
