mod buffer;

pub struct Renderer {
    device: metal::Device,
    queue: metal::CommandQueue,

    layer: metal::MetalLayer,

    pipeline: metal::RenderPipelineState,

    character_vertices: buffer::Buffer<super::Vertex>,
    window_buffer: buffer::Buffer<WindowUniforms>,
    size: crate::window::PhysicalSize,

    glyphs: super::glyph_cache::GlyphCache,
    font_atlas: metal::Texture,
    white_texture: metal::Texture,
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct WindowUniforms {
    size: [f32; 2],
}

const SURFACE_FORMAT: metal::MTLPixelFormat = metal::MTLPixelFormat::BGRA8Unorm;
const TEXTURE_FORMAT: metal::MTLPixelFormat = metal::MTLPixelFormat::RGBA8Unorm;

impl Renderer {
    pub fn new(
        window: &crate::window::cocoa::Window,
        font: std::sync::Arc<crate::font::Font>,
    ) -> Renderer {
        let device = metal::Device::system_default().unwrap();
        let queue = device.new_command_queue();

        let layer = metal::MetalLayer::new();
        layer.set_device(&device);
        layer.set_pixel_format(SURFACE_FORMAT);
        layer.set_presents_with_transaction(false);
        layer.set_framebuffer_only(true);

        unsafe {
            use cocoa::appkit::NSView;

            let view = window.content_view();
            view.setWantsLayer(cocoa::base::YES);
            view.setLayer(std::mem::transmute(layer.as_ref()));
        }

        let inner_size = window.inner_size();
        layer.set_drawable_size(metal::CGSize::new(
            inner_size.width as f64,
            inner_size.height as f64,
        ));

        let library = {
            let source = include_str!("metal/shader.metal");
            let options = metal::CompileOptions::new();
            match device.new_library_with_source(source, &options) {
                Ok(library) => library,
                Err(e) => panic!("failed to compile shader: {}", e),
            }
        };

        let pipeline = {
            let vertex_func = library.get_function("vertex_shader", None).unwrap();
            let fragment_func = library.get_function("fragment_shader", None).unwrap();

            let desc = metal::RenderPipelineDescriptor::new();
            desc.set_vertex_function(Some(&vertex_func));
            desc.set_fragment_function(Some(&fragment_func));

            let attachment = desc.color_attachments().object_at(0).unwrap();
            attachment.set_pixel_format(SURFACE_FORMAT);

            attachment.set_blending_enabled(true);

            attachment.set_rgb_blend_operation(metal::MTLBlendOperation::Add);
            attachment.set_source_rgb_blend_factor(metal::MTLBlendFactor::One);
            attachment.set_destination_rgb_blend_factor(metal::MTLBlendFactor::OneMinusSourceAlpha);

            attachment.set_alpha_blend_operation(metal::MTLBlendOperation::Add);
            attachment.set_source_alpha_blend_factor(metal::MTLBlendFactor::SourceAlpha);
            attachment
                .set_destination_alpha_blend_factor(metal::MTLBlendFactor::OneMinusSourceAlpha);

            device.new_render_pipeline_state(&desc).unwrap()
        };

        let character_vertices = buffer::Buffer::new(0, &device);

        let window_buffer = {
            let uniforms = WindowUniforms {
                size: [inner_size.width as f32, inner_size.height as f32],
            };
            buffer::Buffer::with_data(std::slice::from_ref(&uniforms), &device)
        };

        let font_atlas = {
            let desc = metal::TextureDescriptor::new();

            desc.set_pixel_format(TEXTURE_FORMAT);
            desc.set_usage(metal::MTLTextureUsage::ShaderRead);

            desc.set_texture_type(metal::MTLTextureType::D2);
            desc.set_width(super::FONT_ATLAS_SIZE as u64);
            desc.set_height(super::FONT_ATLAS_SIZE as u64);

            device.new_texture(&desc)
        };

        let white_texture = {
            let desc = metal::TextureDescriptor::new();

            desc.set_pixel_format(TEXTURE_FORMAT);
            desc.set_usage(metal::MTLTextureUsage::ShaderRead);

            desc.set_texture_type(metal::MTLTextureType::D2);
            desc.set_width(1);
            desc.set_height(1);

            let texture = device.new_texture(&desc);
            texture.replace_region(
                metal::MTLRegion::new_2d(0, 0, 1, 1),
                0,
                (&[255u8; 4]).as_ptr() as *const _,
                4,
            );
            texture
        };

        Renderer {
            device,
            queue,
            layer,
            pipeline,
            character_vertices,
            window_buffer,
            size: inner_size,

            glyphs: super::glyph_cache::GlyphCache::new(font, super::FONT_ATLAS_SIZE),
            font_atlas,
            white_texture,
        }
    }

    pub fn resize(&mut self, size: crate::window::PhysicalSize) {
        self.size = size;

        self.layer
            .set_drawable_size(metal::CGSize::new(size.width as f64, size.height as f64));

        self.window_buffer.modify(0..1, |uniforms| {
            uniforms[0].size = [size.width as f32, size.height as f32]
        });
    }

    pub fn set_font(&mut self, font: std::sync::Arc<crate::font::Font>) {
        self.glyphs = super::glyph_cache::GlyphCache::new(font, super::FONT_ATLAS_SIZE);
    }

    pub fn render(&mut self, grid: &crate::grid::CharacterGrid, cursor: super::CursorState) {
        self.update_character_vertices(grid);

        let drawable = self.layer.next_drawable().unwrap();

        let command_buffer = self.queue.new_command_buffer();
        let encoder = Self::create_command_encoder(command_buffer, drawable.texture());

        // Setup rendering pipeline
        encoder.set_render_pipeline_state(&self.pipeline);
        encoder.set_fragment_texture(0, Some(&self.font_atlas));

        self.render_characters(encoder);

        if false {
            self.render_font_atlas(encoder)
        }

        self.render_cursor(encoder, cursor);

        encoder.end_encoding();
        command_buffer.present_drawable(&drawable);
        command_buffer.commit();
    }

    fn create_command_encoder<'a>(
        command_buffer: &'a metal::CommandBufferRef,
        target: &metal::TextureRef,
    ) -> &'a metal::RenderCommandEncoderRef {
        let desc = metal::RenderPassDescriptor::new();

        let attachment = desc.color_attachments().object_at(0).unwrap();
        attachment.set_texture(Some(target));
        attachment.set_clear_color(metal::MTLClearColor::new(0.0, 0.0, 0.0, 1.0));
        attachment.set_load_action(metal::MTLLoadAction::Clear);
        attachment.set_store_action(metal::MTLStoreAction::Store);

        command_buffer.new_render_command_encoder(&desc)
    }

    fn render_characters(&self, encoder: &metal::RenderCommandEncoderRef) {
        encoder.set_vertex_buffers(
            0,
            &[Some(&self.character_vertices), Some(&self.window_buffer)],
            &[0; 2],
        );
        encoder.draw_primitives(
            metal::MTLPrimitiveType::Triangle,
            0,
            self.character_vertices.len() as u64,
        );
    }

    fn render_font_atlas(&self, encoder: &metal::RenderCommandEncoderRef) {
        let atlas_vertices = self.create_atlas_vertices();
        encoder.set_vertex_buffers(
            0,
            &[Some(&atlas_vertices), Some(&self.window_buffer)],
            &[0; 2],
        );
        encoder.draw_primitives(
            metal::MTLPrimitiveType::Triangle,
            0,
            atlas_vertices.len() as u64,
        );
    }

    fn create_atlas_vertices(&self) -> buffer::Buffer<super::Vertex> {
        let width = self.size.width as f32;
        let height = self.size.height as f32;

        let vertices = super::Vertex::quad(
            [width as f32 - 256.0, width, height - 256.0, height],
            [0.0, 1.0, 0.0, 1.0],
            [1.0; 4],
        );

        buffer::Buffer::with_data(&vertices, &self.device)
    }

    fn render_cursor(&self, encoder: &metal::RenderCommandEncoderRef, cursor: super::CursorState) {
        let cursor_vertices = self.create_cursor_vertices(cursor);
        encoder.set_fragment_texture(0, Some(&self.white_texture));
        encoder.set_vertex_buffers(
            0,
            &[Some(&cursor_vertices), Some(&self.window_buffer)],
            &[0; 2],
        );
        encoder.draw_primitives(
            metal::MTLPrimitiveType::Triangle,
            0,
            cursor_vertices.len() as u64,
        );
    }

    fn create_cursor_vertices(&self, cursor: super::CursorState) -> buffer::Buffer<super::Vertex> {
        let [width, height] = crate::font::cell_size(self.glyphs.font());

        let x = cursor.position.col as f32 * width;
        let y = cursor.position.row as f32 * height;

        let vertices = super::Vertex::quad(
            [x, x + width, y, y + height],
            [0.0, 1.0, 0.0, 1.0],
            [1.0; 4],
        );

        buffer::Buffer::with_data(&vertices, &self.device)
    }

    // TODO: do this in a compute shader instead
    fn update_character_vertices(&mut self, grid: &crate::grid::CharacterGrid) {
        let cols = grid.cols();
        let rows = grid.rows();

        let mut quads = Vec::<[super::Vertex; 6]>::with_capacity(cols as usize * rows as usize);

        let font_metrics = *self.glyphs.font().metrics();

        for row in 0..rows {
            for col in 0..cols {
                let cell = grid[crate::grid::Position::new(row, col)];
                let glyph = self.get_glyph(cell.character);
                let foreground = cell.foreground.into_rgba_f32();

                let baseline_x = col as f32 * font_metrics.advance;
                let baseline_y = (1 + row) as f32 * font_metrics.line_height - font_metrics.descent;

                let quad = super::Vertex::glyph_quad(glyph, [baseline_x, baseline_y], foreground);

                quads.push(quad);
            }
        }

        self.character_vertices
            .update(bytemuck::cast_slice(&quads), &self.device);
    }

    fn get_glyph(&mut self, ch: char) -> super::glyph_cache::Glyph {
        self.glyphs.get(ch).unwrap_or_else(|| {
            let (glyph, pixels) = self.glyphs.rasterize(ch).unwrap();

            let region = metal::MTLRegion::new_2d(
                glyph.offset[0] as u64,
                glyph.offset[1] as u64,
                glyph.size[0] as u64,
                glyph.size[1] as u64,
            );

            self.font_atlas.replace_region(
                region,
                0,
                pixels.as_ptr() as *const _,
                4 * glyph.size[0] as u64,
            );

            glyph
        })
    }
}
