mod font;
mod grid;
mod inline_str;
mod render;
mod screen;
mod tty;
mod window;
mod color;

use std::sync::Arc;

#[derive(Debug)]
struct Foo;

impl Drop for Foo {
    fn drop(&mut self) {
        eprintln!("drop Foo")
    }
}

fn main() {
    let link = tty::PsuedoterminalLink::create().unwrap();

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
    let pty = tty::Psuedoterminal::connect(link, waker);
    pty.set_grid_size(screen.grid.size());

    let mut terminal = Terminal {
        pty,
        window,
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

pub struct Terminal {
    pty: tty::Psuedoterminal,
    window: window::Window,
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
            self.screen.grid = grid::CharacterGrid::new(new_grid_size[0], new_grid_size[1]);
            self.screen.cursor = grid::Position::new(0, 0);
        }
    }

    pub fn scale_factor_changed(&mut self) {
        self.font = Arc::new(load_font(self.window.scale_factor()));
        self.renderer.set_font(self.font.clone());
    }

    pub fn key_press(&mut self, key: window::Key, modifiers: window::Modifiers) {
        match key {
            window::Key::Char(ch) => {
                use window::Modifiers;
                if modifiers.is_empty() || modifiers == Modifiers::SHIFT {
                    self.pty.send_char(ch);
                } else if modifiers.contains(Modifiers::CONTROL) && ch.is_ascii_alphabetic() {
                    let byte = (ch as u8).to_ascii_lowercase();
                    self.pty.send_char((byte - b'a' + 1) as char);
                } else {
                    eprintln!("{:?} (modifiers = {:?})", ch, modifiers)
                }
            }

            window::Key::Escape => self.pty.send_char('\x1b'),

            window::Key::Enter => self.pty.send_char('\n'),
            window::Key::Backspace => self.pty.send_char('\x08'),
            window::Key::Tab => self.pty.send_char('\t'),

            window::Key::ArrowUp => write!(self.pty, "\x1b[A"),
            window::Key::ArrowDown => write!(self.pty, "\x1b[B"),
            window::Key::ArrowRight => write!(self.pty, "\x1b[C"),
            window::Key::ArrowLeft => write!(self.pty, "\x1b[D"),
        }
    }

    pub fn poll_input(&mut self) {
        loop {
            match self.pty.try_read() {
                Ok(input) => self.screen.process_input(&input),
                Err(tty::TryReadError::Empty) => break,
                Err(tty::TryReadError::Closed) => {
                    self.window.close();
                    return;
                }
            }
        }
    }

    pub fn render(&mut self) {
        self.renderer.render(
            &self.screen.grid,
            render::CursorState {
                position: self.screen.cursor,
            },
        );
    }
}
