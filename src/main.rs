mod font;
mod grid;
mod render;
mod tty;
mod window;

use std::sync::Arc;

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

    let font = Arc::new(load_font(window.scale_factor()));
    let mut renderer = render::Renderer::new(&window, font.clone());

    let grid_size = grid::size_in_window(window.inner_size(), font::cell_size(&font));
    let mut grid = grid::CharacterGrid::new(grid_size[0], grid_size[1]);
    let mut cursor = grid::Position::new(0, 0);

    let waker = event_loop.create_waker();
    let terminal = tty::Terminal::connect(pty, waker);
    terminal.set_grid_size(grid.size());

    event_loop.run(move |event| match event {
        window::Event::Active => {}
        window::Event::Inactive => {}
        window::Event::Resize(size) => {
            eprintln!("resize: {}x{}", size.width, size.height);

            renderer.resize(size);

            let old_grid_size = grid.size();
            let new_grid_size = grid::size_in_window(size, font::cell_size(&font));

            if old_grid_size != new_grid_size {
                terminal.set_grid_size(new_grid_size);
                grid = grid::CharacterGrid::new(new_grid_size[0], new_grid_size[1]);
                cursor = grid::Position::new(0, 0);
            }
        }
        window::Event::ScaleFactorChanged => {
            let font = Arc::new(load_font(window.scale_factor()));
            renderer.set_font(font);
            renderer.resize(window.inner_size());
        }
        window::Event::KeyPress(key, modifiers) => match key {
            window::Key::Char(ch) => {
                if modifiers.is_empty() || modifiers == window::Modifiers::SHIFT {
                    terminal.send_char(ch);
                } else if modifiers.contains(window::Modifiers::CONTROL) && ch.is_ascii_alphabetic()
                {
                    let byte = (ch as u8).to_ascii_lowercase();
                    terminal.send_char((byte - b'a' + 1) as char);
                } else {
                    eprintln!("{:?} (modifiers = {:?})", ch, modifiers)
                }
            }

            window::Key::Escape => terminal.send_char('\x1b'),

            window::Key::Enter => terminal.send_char('\n'),
            window::Key::Backspace => terminal.send_char('\x08'),
            window::Key::Tab => terminal.send_char('\t'),

            window::Key::ArrowUp => write!(terminal, "\x1b[A"),
            window::Key::ArrowDown => write!(terminal, "\x1b[B"),
            window::Key::ArrowRight => write!(terminal, "\x1b[C"),
            window::Key::ArrowLeft => write!(terminal, "\x1b[D"),
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
                        tty::TerminalCode::Unknown(sequence) => {
                            // eprintln!("unknown control sequence: {:?}", sequence)
                        }

                        tty::TerminalCode::Ignored(_sequence) => {
                            // eprintln!("ignored control sequence: {:?}", sequence)
                        }

                        tty::TerminalCode::Char(ch) => {
                            grid[cursor].character = ch;
                            if increment_wrapping(&mut cursor.col, grid.cols()) {
                                if cursor.row == grid.max_row() {
                                    grid.scroll_up(1);
                                } else {
                                    cursor.row += 1;
                                }
                            }
                        }
                        tty::TerminalCode::Text(text) => {
                            for ch in text.chars() {
                                grid[cursor].character = ch;
                                if increment_wrapping(&mut cursor.col, grid.cols()) {
                                    if cursor.row == grid.max_row() {
                                        grid.scroll_up(1);
                                    } else {
                                        cursor.row += 1;
                                    }
                                }
                            }
                        }
                        tty::TerminalCode::CarriageReturn => {
                            cursor.col = 0;
                        }
                        tty::TerminalCode::LineFeed => {
                            cursor.col = 0;
                            if cursor.row < grid.rows() - 1 {
                                cursor.row += 1;
                            } else {
                                grid.scroll_up(1);
                            }
                        }
                        tty::TerminalCode::Backspace => {
                            if cursor.col > 0 {
                                cursor.col -= 1;
                            } else {
                                cursor.col = grid.cols() - 1;
                            }
                        }
                        tty::TerminalCode::Bell => {
                            eprintln!("Bell!!!")
                        }
                        tty::TerminalCode::Tab => {
                            let width = grid.cols();
                            loop {
                                increment_wrapping(&mut cursor.col, width);
                                if cursor.col % 8 == 0 {
                                    break;
                                }
                            }
                        }
                        tty::TerminalCode::MoveCursor { direction, steps } => match direction {
                            tty::Direction::Up => cursor.row = cursor.row.saturating_sub(steps),
                            tty::Direction::Down => {
                                cursor.row = cursor.row.saturating_add(steps).min(grid.max_row())
                            }
                            tty::Direction::Left => cursor.col = cursor.col.saturating_sub(steps),
                            tty::Direction::Right => {
                                cursor.col = cursor.col.saturating_add(steps).min(grid.max_col())
                            }
                        },
                        tty::TerminalCode::SetCursorPos(new_pos) => {
                            cursor.col = new_pos[0].min(grid.max_row());
                            cursor.row = new_pos[1].min(grid.max_col());
                        }

                        tty::TerminalCode::Erase { count } => {
                            // TODO: figure out what this actually is supposed to do
                            dbg!(code);
                        }

                        tty::TerminalCode::ClearScreenToEnd => {
                            // clear current line
                            grid.clear_region(cursor.row..=cursor.row, cursor.col..);
                            // clear everything after current line
                            grid.clear_region(cursor.row + 1.., ..);
                        }
                        tty::TerminalCode::ClearScreenToStart => {
                            // clear current line
                            grid.clear_region(cursor.row..=cursor.row, ..cursor.col);
                            // clear everything before current line
                            grid.clear_region(..cursor.row, ..);
                        }
                        tty::TerminalCode::ClearScreen => {
                            grid.clear_region(.., ..);
                        }
                        tty::TerminalCode::ClearScreenAndScrollback => {
                            grid.clear_region(.., ..);
                            todo!("clear scrollback")
                        }

                        tty::TerminalCode::ClearLineToEnd => {
                            grid.clear_region(cursor.row..=cursor.row, cursor.col..)
                        }
                        tty::TerminalCode::ClearLineToStart => {
                            grid.clear_region(cursor.row..=cursor.row, ..=cursor.col)
                        }
                        tty::TerminalCode::ClearLine => {
                            grid.clear_region(cursor.row..=cursor.row, ..)
                        }

                        tty::TerminalCode::SetWindowTitle(title) => window.set_title(&title),

                        tty::TerminalCode::SetBracketedPaste(_enabled) => {
                            eprintln!("TODO: bracketed paste");
                        }

                        tty::TerminalCode::SetApplicationCursor(_enabled) => {
                            eprintln!("TODO: application cursor sequences");
                        }
                    }
                }
            }

            renderer.render(&grid, render::CursorState { position: cursor });
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
