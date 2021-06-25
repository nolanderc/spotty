mod color;
mod font;
mod grid;
mod inline;
mod render;
mod screen;
mod tty;
mod window;

#[macro_use]
extern crate tracing;

use std::sync::Arc;

#[derive(Debug)]
struct Foo;

impl Drop for Foo {
    fn drop(&mut self) {
        eprintln!("drop Foo")
    }
}

fn main() {
    setup_logging();

    let event_loop = window::EventLoop::new();
    let window = window::Window::new(
        &event_loop,
        window::WindowConfig {
            size: window::PhysicalSize::new(800, 600),
        },
    );

    let mut terminal = Terminal::new(window, event_loop.create_waker());

    event_loop.run(move |event| match event {
        window::Event::Active => {}
        window::Event::Inactive => {}
        window::Event::Resize(size) => terminal.resize(size),
        window::Event::ScaleFactorChanged => terminal.scale_factor_changed(),
        window::Event::KeyPress(key, modifiers) => terminal.key_press(key, modifiers),
        window::Event::EventsCleared => {
            terminal.poll_input();
            terminal.render();
        }
    });
}

fn load_font(font_size: f64, scale_factor: f64) -> font::Font {
    font::Font::with_name("Iosevka SS14", font_size * scale_factor).expect("failed to load font")
}

fn setup_logging() {
    use tracing_subscriber::{EnvFilter, FmtSubscriber};
    let env_filter = EnvFilter::new(std::env::var("RUST_LOG").as_deref().unwrap_or("info"));
    FmtSubscriber::builder().with_env_filter(env_filter).init();
}

pub struct Terminal {
    pty: tty::Psuedoterminal,

    window: window::Window,
    waker: window::EventLoopWaker,

    renderer: render::Renderer,

    font: Arc<font::Font>,
    font_size: f64,

    screen: screen::Screen,

    dirty: bool,
}

impl Terminal {
    pub fn new(window: window::Window, waker: window::EventLoopWaker) -> Terminal {
        let font_size = 14.0;

        let font = Arc::new(load_font(font_size, window.scale_factor()));
        let renderer = render::Renderer::new(&window, font.clone());

        let grid_size = grid::size_in_window(window.inner_size(), font::cell_size(&font));
        let screen = screen::Screen::new(grid_size);

        let pty = tty::Psuedoterminal::connect(waker.clone()).unwrap();
        pty.set_grid_size(screen.grid.size());

        Terminal {
            pty,

            window,
            waker,

            renderer,

            font,
            font_size,

            screen,

            dirty: true,
        }
    }

    pub fn resize(&mut self, size: window::PhysicalSize) {
        eprintln!("resize: {}x{}", size.width, size.height);

        self.renderer.resize(size);
        self.update_grid_size(size);
        self.dirty = true;
    }

    fn update_grid_size(&mut self, window_size: window::PhysicalSize) {
        let old_grid_size = self.screen.grid.size();
        let new_grid_size = grid::size_in_window(window_size, font::cell_size(&self.font));

        if old_grid_size != new_grid_size {
            self.pty.set_grid_size(new_grid_size);
            self.screen.resize_grid(new_grid_size);
            self.dirty = true;
        }
    }

    pub fn scale_factor_changed(&mut self) {
        self.reload_font();
        self.resize(self.window.inner_size());
    }

    fn reload_font(&mut self) {
        self.font = Arc::new(load_font(self.font_size, self.window.scale_factor()));
        self.renderer.set_font(self.font.clone());
        self.update_grid_size(self.window.inner_size());
        self.dirty = true;
    }

    pub fn key_press(&mut self, key: window::Key, modifiers: window::Modifiers) {
        use window::Modifiers;

        match key {
            window::Key::Char(ch) => match modifiers {
                Modifiers::EMPTY | Modifiers::SHIFT => {
                    let mut buffer = [0u8; 4];
                    let encoded = ch.encode_utf8(&mut buffer);
                    self.pty.send(encoded.as_bytes());
                }
                _ if modifiers.contains(Modifiers::CONTROL) && ch.is_ascii_alphabetic() => {
                    let byte = (ch as u8).to_ascii_lowercase();
                    self.pty.send(byte - b'a' + 1);
                }
                _ if modifiers.contains(Modifiers::ALT) && ch.is_ascii_alphabetic() => {
                    self.pty.send([0x1b, ch as u8]);
                }
                _ => match (modifiers, ch) {
                    (Modifiers::SUPER, 'v') => self.paste_clipboard(),
                    (Modifiers::SUPER, '-') => self.decrease_font_size(),
                    (Modifiers::SUPER, '=') => self.increase_font_size(),
                    _ => {
                        eprintln!("{:?} (modifiers = {:?})", ch, modifiers);
                        return;
                    }
                },
            },

            window::Key::Escape => self.pty.send(b"\x1b"),

            window::Key::Enter => {
                if modifiers.contains(Modifiers::ALT) {
                    self.pty.send(b"\x1b\r")
                } else {
                    self.pty.send(b"\r")
                }
            }
            window::Key::Backspace => self.pty.send(b"\x08"),
            window::Key::Tab => self.pty.send(b"\t"),

            window::Key::ArrowUp => self.pty.send(b"\x1b[A"),
            window::Key::ArrowDown => self.pty.send(b"\x1b[B"),
            window::Key::ArrowRight => self.pty.send(b"\x1b[C"),
            window::Key::ArrowLeft => self.pty.send(b"\x1b[D"),
        }

        self.dirty = true;
    }

    fn decrease_font_size(&mut self) {
        self.font_size = f64::max(6.0, self.font_size / 1.25);
        self.reload_font();
    }

    fn increase_font_size(&mut self) {
        self.font_size = f64::max(6.0, self.font_size * 1.25);
        self.reload_font();
    }

    fn paste_clipboard(&mut self) {
        if let Some(clipboard) = self.window.get_clipboard() {
            let escaped = clipboard.replace('\x1b', "");
            let bytes = escaped.into_boxed_str().into_boxed_bytes();
            if self.screen.behaviours.bracketed_paste {
                self.pty.send(b"\x1b[200~");
                self.pty.send(bytes);
                self.pty.send(b"\x1b[201~");
            } else {
                self.pty.send(bytes);
            }
        }
    }

    pub fn poll_input(&mut self) {
        let start_poll = std::time::Instant::now();
        let max_poll_duration = std::time::Duration::from_millis(10);

        loop {
            match self.pty.read_timeout(std::time::Duration::from_millis(1)) {
                Ok(input) => {
                    self.screen.process_input(&input);
                    self.dirty = true;
                }
                Err(tty::TryReadError::Empty) => break,
                Err(tty::TryReadError::Closed) => {
                    self.window.close();
                    return;
                }
            }

            if start_poll.elapsed() > max_poll_duration {
                self.waker.wake();
                break;
            }
        }
    }

    pub fn render(&mut self) {
        if self.dirty {
            let palette = &crate::color::DEFAULT_PALETTE;

            let cursor = self.screen.cursor_render_state(palette);

            let start = std::time::Instant::now();

            self.renderer.render(render::RenderState {
                grid: &self.screen.grid,
                cursor,
                palette,
            });

            eprintln!("render: {:.2} ms", start.elapsed().as_secs_f64() * 1e3);

            self.dirty = false;
        }
    }
}
