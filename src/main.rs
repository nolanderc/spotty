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

    let font = Arc::new(load_font(window.scale_factor()));
    let renderer = render::Renderer::new(&window, font.clone());

    let grid_size = grid::size_in_window(window.inner_size(), font::cell_size(&font));
    let screen = screen::Screen::new(grid_size);

    let waker = event_loop.create_waker();
    let pty = tty::Psuedoterminal::connect(waker).unwrap();
    pty.set_grid_size(screen.grid.size());

    let mut terminal = Terminal {
        pty,
        window,
        waker: event_loop.create_waker(),
        renderer,
        font,
        screen,
    };

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

fn load_font(scale_factor: f64) -> font::Font {
    let font_size = 16.0;
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

    screen: screen::Screen,
}

impl Terminal {
    pub fn resize(&mut self, size: window::PhysicalSize) {
        eprintln!("resize: {}x{}", size.width, size.height);

        self.renderer.resize(size);

        let old_grid_size = self.screen.grid.size();
        let new_grid_size = grid::size_in_window(size, font::cell_size(&self.font));

        if old_grid_size != new_grid_size {
            self.pty.set_grid_size(new_grid_size);
            self.screen.resize_grid(new_grid_size);
        }
    }

    pub fn scale_factor_changed(&mut self) {
        self.font = Arc::new(load_font(self.window.scale_factor()));
        self.renderer.set_font(self.font.clone());
        self.resize(self.window.inner_size());
    }

    pub fn key_press(&mut self, key: window::Key, modifiers: window::Modifiers) {
        match key {
            window::Key::Char(ch) => {
                use window::Modifiers;
                if modifiers.is_empty() || modifiers == Modifiers::SHIFT {
                    let mut buffer = [0u8; 4];
                    let encoded = ch.encode_utf8(&mut buffer);
                    self.pty.send(encoded.as_bytes());
                } else if modifiers.contains(Modifiers::CONTROL) && ch.is_ascii_alphabetic() {
                    let byte = (ch as u8).to_ascii_lowercase();
                    self.pty.send(byte - b'a' + 1);
                } else {
                    eprintln!("{:?} (modifiers = {:?})", ch, modifiers)
                }
            }

            window::Key::Escape => self.pty.send(b"\x1b"),

            window::Key::Enter => self.pty.send(b"\r"),
            window::Key::Backspace => self.pty.send(b"\x08"),
            window::Key::Tab => self.pty.send(b"\t"),

            window::Key::ArrowUp => self.pty.send(b"\x1b[A"),
            window::Key::ArrowDown => self.pty.send(b"\x1b[B"),
            window::Key::ArrowRight => self.pty.send(b"\x1b[C"),
            window::Key::ArrowLeft => self.pty.send(b"\x1b[D"),
        }
    }

    pub fn poll_input(&mut self) {
        let start_poll = std::time::Instant::now();
        let max_poll_duration = std::time::Duration::from_millis(10);

        loop {
            match self.pty.read_timeout(std::time::Duration::from_millis(1)) {
                Ok(input) => self.screen.process_input(&input),
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
        self.renderer.render(
            &self.screen.grid,
            if self.screen.behaviours.show_cursor {
                Some(render::CursorState {
                    position: self.screen.cursor,
                })
            } else {
                None
            },
        );
    }
}
