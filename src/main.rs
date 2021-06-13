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
    let pty = tty::Psuedoterminal::create().unwrap();

    let event_loop = window::EventLoop::new();
    let window = window::Window::new(
        &event_loop,
        window::WindowConfig {
            size: window::PhysicalSize::new(800, 600),
        },
    );

    let mut renderer = render::Renderer::new(&window, load_font(window.scale_factor()));
    let mut needs_redraw = true;
    let mut cursor = [0, 0];

    let waker = event_loop.create_waker();
    let terminal = tty::Terminal::connect(pty, waker);
    terminal.set_grid_size(renderer.grid().size());

    event_loop.run(move |event| match event {
        window::Event::Active => {}
        window::Event::Inactive => {}
        window::Event::Resize(size) => {
            let old_grid_size = renderer.grid().size();
            renderer.resize(size);
            let new_grid_size = renderer.grid().size();

            if old_grid_size != new_grid_size {
                terminal.set_grid_size(new_grid_size);
                cursor = [0, 0];
            }

            needs_redraw = true;
        }
        window::Event::Char(ch) => terminal.send(ch),
        window::Event::ScaleFactorChanged => {
            let font = load_font(window.scale_factor());
            renderer.set_font(font);
            renderer.resize(window.inner_size());
            needs_redraw = true;
        }
        window::Event::KeyPress(key) => match key {
            window::Key::Enter => terminal.send('\n'),
            window::Key::Backspace => terminal.send('\x08'),
            window::Key::Tab => terminal.send('\t'),
        },
        window::Event::EventsCleared => {
            loop {
                let message = match terminal.try_read() {
                    Ok(message) => message,
                    Err(tty::TryReadError::Empty) => break,
                    Err(tty::TryReadError::Closed) => {
                        window.close();
                        return;
                    }
                };

                for code in message {
                    match code {
                        tty::TerminalCode::Char(ch) => {
                            let grid = renderer.grid();
                            grid[cursor].character = ch;
                            if increment_wrapping(&mut cursor[0], grid.width()) {
                                increment_wrapping(&mut cursor[1], grid.height());
                            }
                        }
                        tty::TerminalCode::CarriageReturn => {
                            cursor[0] = 0;
                        }
                        tty::TerminalCode::LineFeed => {
                            let grid = renderer.grid();
                            if cursor[1] < grid.height() - 1 {
                                cursor[1] += 1;
                            } else {
                                grid.scroll_up(1);
                            }
                        }
                        tty::TerminalCode::Backspace => {
                            if cursor[0] > 0 {
                                cursor[0] -= 1;
                            } else {
                                cursor[0] = renderer.grid().width() - 1;
                            }
                        }
                        tty::TerminalCode::ClearLineToEnd => renderer
                            .grid()
                            .clear_region(cursor[0].., cursor[1]..=cursor[1]),
                        tty::TerminalCode::ClearLineToStart => renderer
                            .grid()
                            .clear_region(..=cursor[0], cursor[1]..=cursor[1]),
                        tty::TerminalCode::ClearLine => {
                            renderer.grid().clear_region(.., cursor[1]..=cursor[1])
                        }
                        tty::TerminalCode::Bell => {
                            eprintln!("Bell!!!")
                        }
                        tty::TerminalCode::Tab => {
                            let width = renderer.grid().width();
                            loop {
                                increment_wrapping(&mut cursor[0], width);
                                if cursor[0] % 8 == 0 {
                                    break;
                                }
                            }
                        }
                    }
                }

                needs_redraw = true;
            }

            if needs_redraw {
                needs_redraw = false;
                renderer.set_cursor_position(cursor);
                renderer.render();
            }
        }
    });
}

fn load_font(scale_factor: f64) -> font::Font {
    let font_size = 16.0;
    font::Font::with_name("Iosevka SS14", font_size * scale_factor).expect("failed to load font")
}

fn increment_wrapping(v: &mut u16, max: u16) -> bool {
    *v += 1;
    if *v < max {
        false
    } else {
        *v = 0;
        true
    }
}
