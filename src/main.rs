mod font;
mod render;
mod tty;
mod window;

#[derive(Debug)]
struct Foo;

impl Drop for Foo {
    fn drop(&mut self) {
        eprintln!("drop Foo")
    }
}

fn main() {
    let tty = tty::Psuedoterminal::create().unwrap();

    let event_loop = window::EventLoop::new();
    let window = window::Window::new(
        &event_loop,
        window::WindowConfig {
            size: window::PhysicalSize::new(800, 600),
        },
    );

    let mut cursor = [0, 0];

    let mut renderer = render::Renderer::new(&window, load_font(window.scale_factor()));
    renderer.grid()[cursor].character = '😅';

    let mut needs_redraw = true;

    let (terminal_input, mut terminal_output) = {
        use std::os::unix::io::{AsRawFd, FromRawFd};

        unsafe {
            let input =
                std::fs::File::from_raw_fd(nix::unistd::dup(tty.master_fd.as_raw_fd()).unwrap());
            let output =
                std::fs::File::from_raw_fd(nix::unistd::dup(tty.master_fd.as_raw_fd()).unwrap());

            (input, std::io::BufReader::new(output))
        }
    };

    std::thread::spawn(move || {
        use std::io::Read;
        let mut buffer = [0; 8 * 1024];
        while let Ok(count) = terminal_output.read(&mut buffer) {
            if count == 0 {
                break;
            }

            let bytes = &buffer[..count];
            dbg!(String::from_utf8_lossy(bytes));
        }
    });

    event_loop.run(move |event| match event {
        window::Event::Active => {}
        window::Event::Inactive => {}
        window::Event::Resize(size) => {
            cursor = [0, 0];
            renderer.resize(size);
            needs_redraw = true;
        }
        window::Event::Char(ch) => {
            let grid = renderer.grid();

            grid[cursor].character = ch;

            fn increment_wrapping(v: &mut u16, max: u16) -> bool {
                *v += 1;
                if *v < max {
                    false
                } else {
                    *v = 0;
                    true
                }
            }

            if increment_wrapping(&mut cursor[0], grid.width()) {
                increment_wrapping(&mut cursor[1], grid.height());
            }

            needs_redraw = true;
        }
        window::Event::ScaleFactorChanged => {
            let font = load_font(window.scale_factor());
            renderer.set_font(font);
            renderer.resize(window.inner_size());
            needs_redraw = true;
        }
        window::Event::EventsCleared => {
            if needs_redraw {
                needs_redraw = false;
                renderer.render();
            }
        }
    });
}

fn load_font(scale_factor: f64) -> font::Font {
    let font_size = 24.0;
    font::Font::with_name("Iosevka SS14", font_size * scale_factor).expect("failed to load font")
}
