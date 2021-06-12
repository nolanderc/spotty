mod buffer;

pub struct Renderer {
    device: metal::Device,
    queue: metal::CommandQueue,

    layer: metal::MetalLayer,

    pipeline: metal::RenderPipelineState,

    vertex_buffer: buffer::Buffer<super::Vertex>,
    window_buffer: buffer::Buffer<WindowUniforms>,

    glyphs: super::glyph_cache::GlyphCache,
    texture: metal::Texture,

    grid: super::CharacterGrid,
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct WindowUniforms {
    size: [f32; 2],
}

const SURFACE_FORMAT: metal::MTLPixelFormat = metal::MTLPixelFormat::BGRA8Unorm;
const TEXTURE_FORMAT: metal::MTLPixelFormat = metal::MTLPixelFormat::RGBA8Unorm;

impl Renderer {
    pub fn new(window: &crate::window::cocoa::Window, font: crate::font::Font) -> Renderer {
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

        let grid = {
            let metrics = font.metrics();
            super::CharacterGrid::in_window(inner_size, [metrics.advance, metrics.line_height])
        };

        let vertex_buffer =
            buffer::Buffer::new(6 * grid.width as usize * grid.height as usize, &device);

        let window_buffer = {
            let uniforms = WindowUniforms {
                size: [inner_size.width as f32, inner_size.height as f32],
            };
            buffer::Buffer::with_data(std::slice::from_ref(&uniforms), &device)
        };

        let texture = {
            let desc = metal::TextureDescriptor::new();

            desc.set_pixel_format(TEXTURE_FORMAT);
            desc.set_usage(metal::MTLTextureUsage::ShaderRead);

            desc.set_texture_type(metal::MTLTextureType::D2);
            desc.set_width(super::FONT_ATLAS_SIZE as u64);
            desc.set_height(super::FONT_ATLAS_SIZE as u64);

            device.new_texture(&desc)
        };

        Renderer {
            device,
            queue,
            layer,
            pipeline,
            vertex_buffer,
            window_buffer,

            glyphs: super::glyph_cache::GlyphCache::new(font, super::FONT_ATLAS_SIZE),
            texture,

            grid,
        }
    }

    pub fn resize(&mut self, size: crate::window::PhysicalSize) {
        eprintln!("resize: {}x{}", size.width, size.height);
        self.layer
            .set_drawable_size(metal::CGSize::new(size.width as f64, size.height as f64));
        self.window_buffer.modify(0..1, |uniforms| {
            uniforms[0].size = [size.width as f32, size.height as f32]
        });
    }

    pub fn grid(&mut self) -> &mut super::CharacterGrid {
        &mut self.grid
    }

    pub fn set_font(&mut self, font: crate::font::Font) {
        self.glyphs = super::glyph_cache::GlyphCache::new(font, super::FONT_ATLAS_SIZE);
    }

    pub fn render(&mut self) {
        eprintln!("render");

        self.update_vertex_buffer();

        let drawable = self.layer.next_drawable().unwrap();

        let command_buffer = self.queue.new_command_buffer();
        let encoder = {
            let desc = metal::RenderPassDescriptor::new();

            let attachment = desc.color_attachments().object_at(0).unwrap();
            attachment.set_texture(Some(drawable.texture()));
            attachment.set_clear_color(metal::MTLClearColor::new(0.2, 0.2, 0.2, 1.0));
            attachment.set_load_action(metal::MTLLoadAction::Clear);
            attachment.set_store_action(metal::MTLStoreAction::Store);

            command_buffer.new_render_command_encoder(&desc)
        };

        encoder.set_render_pipeline_state(&self.pipeline);
        encoder.set_fragment_texture(0, Some(&self.texture));
        encoder.set_vertex_buffers(
            0,
            &[Some(&self.vertex_buffer), Some(&self.window_buffer)],
            &[0; 2],
        );
        encoder.draw_primitives(
            metal::MTLPrimitiveType::Triangle,
            0,
            self.vertex_buffer.len() as u64,
        );

        encoder.end_encoding();

        command_buffer.present_drawable(&drawable);
        command_buffer.commit();
    }

    fn update_vertex_buffer(&mut self) {
        let mut quads = Vec::<[super::Vertex; 6]>::with_capacity(
            self.grid.width as usize * self.grid.height as usize,
        );

        let font_metrics = *self.glyphs.font().metrics();

        for y in 0..self.grid.height {
            for x in 0..self.grid.width {
                let cell = self.grid[[x, y]];
                let glyph = self.get_glyph(cell.character);

                let baseline_x = x as f32 * font_metrics.advance;
                let baseline_y = (1 + y) as f32 * font_metrics.line_height - font_metrics.descent;

                let quad = super::Vertex::glyph_quad(glyph, [baseline_x, baseline_y], [1.0; 4]);

                quads.push(quad);
            }
        }

        self.vertex_buffer.write(bytemuck::cast_slice(&quads), 0);
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

            self.texture.replace_region(
                region,
                0,
                pixels.as_ptr() as *const _,
                4 * glyph.size[0] as u64,
            );

            glyph
        })
    }
}
