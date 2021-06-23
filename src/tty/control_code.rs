use std::convert::TryFrom;

pub trait Terminal {
    fn invalid_control_sequence(&mut self, bytes: &[u8]);

    /// Output some text to the screen
    fn text(&mut self, text: &str);

    /// Encountered invalid UTF-8
    fn invalid_utf8(&mut self, text: &[u8]);

    /// Makes an audible bell
    fn bell(&mut self);

    // === CURSOR === //

    /// Move cursor to next column that is an multiple of 8
    fn tab(&mut self);

    /// Move cursor to the left, might wrap to previous line
    fn backspace(&mut self);

    /// `\r`
    fn carriage_return(&mut self);
    /// `\n`
    fn line_feed(&mut self);

    fn reverse_line_feed(&mut self);

    fn delete_lines(&mut self, count: u16);

    fn insert_lines(&mut self, count: u16);

    /// Move the cursor in the given direction
    fn move_cursor(&mut self, direction: Direction, steps: u16);

    /// Sets the position of the cursor relative to the top-left corner (0-indexed)
    fn set_cursor_pos(&mut self, row: u16, col: u16);

    /// Saves the current cursor
    fn save_cursor(&mut self);

    /// Restores the saved cursor
    fn restore_cursor(&mut self);

    /// Set the appearance of the cursor
    fn set_cursor_style(&mut self, style: CursorStyle);

    /// Set the color of the cursor
    fn set_cursor_color(&mut self, color: crate::color::Color);

    /// Set the color of the cursor to the default
    fn reset_cursor_color(&mut self);

    // === SCROLLING === //

    /// Set the area within which content should scroll.
    fn set_scrolling_region(&mut self, rows: std::ops::Range<u16>);

    // === CLEARING === //

    /// Clear from cursor to the end of the line
    fn clear_line(&mut self, region: ClearRegion);

    /// Clear entire screen and scrollback buffer
    fn clear_screen(&mut self, region: ClearRegion);

    /// Clear scrollback buffer
    fn clear_scrollback(&mut self);

    fn erase(&mut self, count: u16);

    // === CHARACTER STYLE === //

    /// Set the style of characters
    fn set_character_style(&mut self, style: CharacterStyles);

    /// Set the style of characters
    fn reset_character_style(&mut self, style: CharacterStyles);

    // === COLOR === //

    /// Set the color of the foreground
    fn set_foreground_color(&mut self, color: crate::color::Color);

    /// Reset the foreground to the default color
    fn reset_foreground_color(&mut self);

    /// Set the color of the background
    fn set_background_color(&mut self, color: crate::color::Color);

    /// Reset the background to the default color
    fn reset_background_color(&mut self);

    // === COLOR === //

    /// Set the title of the window
    fn set_window_title(&mut self, text: &str);

    // === BEHAVIOUR === //

    /// If enabled: arrow keys should send application codes instead of ANSI codes
    fn toggle_behaviour(&mut self, behaviour: Behaviour, toggle: Toggle);
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Toggle {
    Enabled,
    Disabled,
}

impl Toggle {
    /// Returns `true` if the toggle is [`Enabled`].
    pub fn is_enabled(&self) -> bool {
        matches!(self, Self::Enabled)
    }
}

#[derive(Debug, Copy, Clone)]
pub enum ClearRegion {
    /// Clear from cursor to the end of the region
    ToEnd,
    /// Clear from the start of the region to the cursor
    ToStart,
    /// Clear everything
    All,
}

#[derive(Debug, Copy, Clone)]
pub struct CursorStyle {
    pub shape: CursorShape,
    pub blink: CursorBlink,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CursorShape {
    Block,
    Bar,
    Underline,
}

#[derive(Debug, Copy, Clone)]
pub enum CursorBlink {
    Blinking,
    Steady,
}

impl CursorStyle {
    pub const DEFAULT: Self = CursorStyle {
        shape: CursorShape::Block,
        blink: CursorBlink::Blinking,
    };

    fn blinking(shape: CursorShape) -> CursorStyle {
        CursorStyle {
            shape,
            blink: CursorBlink::Blinking,
        }
    }

    fn steady(shape: CursorShape) -> CursorStyle {
        CursorStyle {
            shape,
            blink: CursorBlink::Steady,
        }
    }
}

macro_rules! enumeration {
    (
        $(#[$attr:meta])*
        $vis:vis enum $ident:ident : $repr:ident {
            $( $variant:ident = $value:literal ),* $(,)?
        }
    ) => {
        $(#[$attr])*
        #[repr($repr)]
        $vis enum $ident {
            $( $variant = $value ),*
        }

        impl std::convert::TryFrom<$repr> for $ident {
            type Error = ();

            fn try_from(value: $repr) -> Result<$ident, ()> {
                match value {
                    $( $value => Ok($ident::$variant), )*
                    _ => Err(())
                }
            }
        }
    }
}

enumeration! {
    #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
    pub enum Behaviour: u16 {
        ApplicationCursor = 1,
        ShowCursor        = 25,
        AlternateBuffer   = 47,
        FocusEvents       = 1004,
        BracketedPaste    = 2004,
    }
}

#[allow(non_snake_case)]
bitflags::bitflags! {
    pub struct CharacterStyles: u8 {
        const BOLD          = 0x01;
        const FAINT         = 0x02;
        const ITALIC        = 0x04;
        const UNDERLINE     = 0x08;
        const BLINK         = 0x10;
        const INVERSE       = 0x20;
        const INVISIBLE     = 0x40;
        const STRIKETHROUGH = 0x80;
    }
}

#[derive(Debug, Copy, Clone)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

pub type ParseResult<T> = Result<T, ParseError>;

#[derive(Debug, Copy, Clone)]
pub enum ParseError {
    Incomplete,
    Invalid,
}

pub fn parse<'a>(bytes: &'a [u8], terminal: &mut impl Terminal) -> &'a [u8] {
    let mut remaining = bytes;
    let mut assumed_text = bytes;

    loop {
        match remaining {
            [] => return emit_text(assumed_text, terminal),

            [byte, ..] if byte.is_ascii_control() => {
                let text = &assumed_text[..assumed_text.len() - remaining.len()];
                let invalid = emit_text(text, terminal);
                if !invalid.is_empty() {
                    terminal.invalid_utf8(invalid);
                }

                let mut iter = remaining.iter();
                match parse_control_sequence(&mut iter, terminal) {
                    Ok(()) => {}
                    Err(ParseError::Incomplete) => return remaining,
                    Err(ParseError::Invalid) => {
                        let invalid = &remaining[..remaining.len() - iter.len()];
                        terminal.invalid_control_sequence(invalid);
                    }
                }

                remaining = iter.as_slice();
                assumed_text = remaining;
            }
            [_, rest @ ..] => remaining = rest,
        }
    }
}

fn emit_text<'a>(mut bytes: &'a [u8], terminal: &mut impl Terminal) -> &'a [u8] {
    while !bytes.is_empty() {
        match std::str::from_utf8(bytes) {
            Ok(text) => {
                terminal.text(text);
                break;
            }
            Err(error) => {
                let (valid, invalid) = bytes.split_at(error.valid_up_to());

                // SAFETY: `bytes` is valid UTF-8 up until `error.valid_up_to()`
                let text = unsafe { std::str::from_utf8_unchecked(valid) };
                terminal.text(text);

                match error.error_len() {
                    // Reached end of bytes
                    None => return invalid,
                    Some(invalid_len) => {
                        let (invalid, rest) = invalid.split_at(invalid_len);
                        terminal.invalid_utf8(&invalid);
                        bytes = rest;
                    }
                }
            }
        }
    }

    &[]
}

type ByteIter<'iter, 'slice> = &'iter mut std::slice::Iter<'slice, u8>;

fn parse_control_sequence(bytes: ByteIter, terminal: &mut impl Terminal) -> ParseResult<()> {
    match bytes.next().ok_or(ParseError::Incomplete)? {
        b'\x07' => terminal.bell(),
        b'\x08' => terminal.backspace(),
        b'\x09' => terminal.tab(),
        b'\r' => terminal.carriage_return(),
        b'\n' => terminal.line_feed(),
        b'\x1b' => parse_escape_sequence(bytes, terminal)?,
        _ => return Err(ParseError::Invalid),
    }

    Ok(())
}

fn parse_escape_sequence(bytes: ByteIter, terminal: &mut impl Terminal) -> ParseResult<()> {
    match bytes.next().ok_or(ParseError::Incomplete)? {
        // Control Sequence
        b'[' => parse_escape_control_sequence(bytes, terminal)?,

        // Operating System Command
        b']' => parse_operating_system_command(bytes, terminal)?,

        b'(' => {
            bytes.next().ok_or(ParseError::Incomplete)?;
        }

        b'M' => terminal.reverse_line_feed(),

        _ => return Err(ParseError::Invalid),
    }

    Ok(())
}

fn parse_operating_system_command(
    bytes: ByteIter,
    terminal: &mut impl Terminal,
) -> ParseResult<()> {
    let parameters = util::take_while_in_range(bytes, 0x20..=0x7e)?;
    let terminator = bytes.next().ok_or(ParseError::Incomplete)?;

    if !matches!(terminator, b'\x07' | b'\x03') {
        return Err(ParseError::Invalid);
    }

    let mut arguments = ArgumentList::new(parameters);

    match arguments.next()?.with_default(0) {
        // Change "icon name" and window title. The former does not apply.
        0 => {
            let text = std::str::from_utf8(arguments.next_slice()).unwrap();
            terminal.set_window_title(text);
        }

        // Change "icon name" (does not apply)
        1 => {}

        // Change window title.
        2 => {
            let text = std::str::from_utf8(arguments.next_slice()).unwrap();
            terminal.set_window_title(text);
        }

        // Set X-property on top-level window (does not apply)
        3 => {}

        112 => terminal.reset_cursor_color(),

        _ => return Err(ParseError::Invalid),
    }

    Ok(())
}

fn parse_escape_control_sequence(bytes: ByteIter, terminal: &mut impl Terminal) -> ParseResult<()> {
    let (parameters, intermediate, terminator) = parse_control_sequence_parts(bytes)?;

    match (parameters, intermediate) {
        ([b'?', arguments @ ..], b"") => {
            parse_escape_question_terminator(arguments, terminator, terminal)
        }

        (arguments, b"") => parse_escape_standard_terminator(arguments, terminator, terminal),
        (arguments, b" ") => parse_escape_space_terminator(arguments, terminator, terminal),

        _ => Err(ParseError::Invalid),
    }
}

fn parse_control_sequence_parts<'a>(
    bytes: ByteIter<'_, 'a>,
) -> ParseResult<(&'a [u8], &'a [u8], u8)> {
    let parameters = util::take_while_in_range(bytes, 0x30..=0x3f)?;
    let intermediate = util::take_while_in_range(bytes, 0x20..=0x2f)?;
    let terminator = *bytes.next().ok_or(ParseError::Incomplete)?;

    if (b'\x40'..=b'\x7e').contains(&terminator) {
        Ok((parameters, intermediate, terminator as u8))
    } else {
        Err(ParseError::Invalid)
    }
}

fn parse_escape_question_terminator(
    parameters: &[u8],
    terminator: u8,
    terminal: &mut impl Terminal,
) -> ParseResult<()> {
    match terminator {
        b'h' => parse_question_terminator_toggle(parameters, Toggle::Enabled, terminal),
        b'l' => parse_question_terminator_toggle(parameters, Toggle::Disabled, terminal),
        _ => Err(ParseError::Invalid),
    }
}

fn parse_question_terminator_toggle(
    parameters: &[u8],
    toggle: Toggle,
    terminal: &mut impl Terminal,
) -> ParseResult<()> {
    let argument = Argument::single(parameters)?.with_default(0);

    match Behaviour::try_from(argument) {
        Ok(behaviour) => terminal.toggle_behaviour(behaviour, toggle),

        // Didn't match any behaviour, check for aliases
        Err(()) => match argument {
            1047 => terminal.toggle_behaviour(Behaviour::AlternateBuffer, toggle),

            1048 if toggle.is_enabled() => terminal.save_cursor(),
            1048 => terminal.restore_cursor(),

            1049 if toggle.is_enabled() => {
                terminal.save_cursor();
                terminal.toggle_behaviour(Behaviour::AlternateBuffer, toggle);
                terminal.clear_screen(ClearRegion::All);
            }
            1049 => {
                terminal.toggle_behaviour(Behaviour::AlternateBuffer, toggle);
                terminal.restore_cursor();
            }

            _ => return Err(ParseError::Invalid),
        },
    }

    Ok(())
}

fn parse_escape_standard_terminator(
    parameters: &[u8],
    terminator: u8,
    terminal: &mut impl Terminal,
) -> ParseResult<()> {
    use Direction::{Down, Left, Right, Up};

    match terminator {
        b'm' => parse_character_attribute(parameters, terminal)?,

        b'A' => terminal.move_cursor(Up, Argument::single(parameters)?.with_default(1)),
        b'B' => terminal.move_cursor(Down, Argument::single(parameters)?.with_default(1)),
        b'C' => terminal.move_cursor(Right, Argument::single(parameters)?.with_default(1)),
        b'D' => terminal.move_cursor(Left, Argument::single(parameters)?.with_default(1)),

        b'H' => {
            let [row, col] = Argument::multi(parameters)?;
            terminal.set_cursor_pos(row.with_default(1) - 1, col.with_default(1) - 1)
        }

        b'J' => match Argument::single(parameters)?.with_default(0) {
            0 => terminal.clear_screen(ClearRegion::ToEnd),
            1 => terminal.clear_screen(ClearRegion::ToStart),
            2 => terminal.clear_screen(ClearRegion::All),
            3 => terminal.clear_scrollback(),
            _ => return Err(ParseError::Invalid),
        },

        b'K' => match Argument::single(parameters)?.with_default(0) {
            0 => terminal.clear_line(ClearRegion::ToEnd),
            1 => terminal.clear_line(ClearRegion::ToStart),
            2 => terminal.clear_line(ClearRegion::All),
            _ => return Err(ParseError::Invalid),
        },

        b'L' => terminal.insert_lines(Argument::single(parameters)?.with_default(1)),
        b'M' => terminal.delete_lines(Argument::single(parameters)?.with_default(1)),

        b'X' => terminal.erase(Argument::single(parameters)?.with_default(1)),

        b'r' => {
            let [top, bottom] = Argument::multi(parameters)?;
            let top = top.with_default(1) - 1;
            let bottom = bottom.to_option().unwrap_or(u16::MAX);
            terminal.set_scrolling_region(top..bottom);
        }

        _ => return Err(ParseError::Invalid),
    }

    Ok(())
}

fn parse_escape_space_terminator(
    params: &[u8],
    terminator: u8,
    terminal: &mut impl Terminal,
) -> ParseResult<()> {
    match terminator {
        b'q' => match Argument::single(params)?.with_default(0) {
            0 | 1 => terminal.set_cursor_style(CursorStyle::blinking(CursorShape::Block)),
            2 => terminal.set_cursor_style(CursorStyle::steady(CursorShape::Block)),

            3 => terminal.set_cursor_style(CursorStyle::blinking(CursorShape::Underline)),
            4 => terminal.set_cursor_style(CursorStyle::steady(CursorShape::Underline)),

            5 => terminal.set_cursor_style(CursorStyle::blinking(CursorShape::Bar)),
            6 => terminal.set_cursor_style(CursorStyle::steady(CursorShape::Bar)),

            _ => return Err(ParseError::Invalid),
        },
        _ => return Err(ParseError::Invalid),
    }

    Ok(())
}

fn parse_character_attribute(parameters: &[u8], terminal: &mut impl Terminal) -> ParseResult<()> {
    use crate::color::Color;

    let mut arguments = ArgumentList::new(parameters);
    loop {
        match arguments.next()?.with_default(0) {
            0 => {
                terminal.reset_character_style(CharacterStyles::all());
                terminal.reset_foreground_color();
                terminal.reset_background_color();
            }

            1 => terminal.set_character_style(CharacterStyles::BOLD),
            21 => terminal.reset_character_style(CharacterStyles::BOLD),

            2 => terminal.set_character_style(CharacterStyles::FAINT),
            22 => terminal.reset_character_style(CharacterStyles::FAINT),

            3 => terminal.set_character_style(CharacterStyles::ITALIC),
            23 => terminal.reset_character_style(CharacterStyles::ITALIC),

            4 => terminal.set_character_style(CharacterStyles::UNDERLINE),
            24 => terminal.reset_character_style(CharacterStyles::UNDERLINE),

            5 => terminal.set_character_style(CharacterStyles::BLINK),
            25 => terminal.reset_character_style(CharacterStyles::BLINK),

            7 => terminal.set_character_style(CharacterStyles::INVERSE),
            27 => terminal.reset_character_style(CharacterStyles::INVERSE),

            8 => terminal.set_character_style(CharacterStyles::INVISIBLE),
            28 => terminal.reset_character_style(CharacterStyles::INVISIBLE),

            9 => terminal.set_character_style(CharacterStyles::STRIKETHROUGH),
            29 => terminal.reset_character_style(CharacterStyles::STRIKETHROUGH),

            // Indexed color
            arg @ 30..=37 => terminal.set_foreground_color(Color::Index(arg as u8 - 30)),
            arg @ 90..=97 => terminal.set_foreground_color(Color::Index(8 + arg as u8 - 90)),
            39 => terminal.reset_foreground_color(),

            arg @ 40..=47 => terminal.set_background_color(Color::Index(arg as u8 - 30)),
            arg @ 100..=107 => terminal.set_background_color(Color::Index(8 + arg as u8 - 100)),
            49 => terminal.reset_background_color(),

            // RGB color
            38 => match arguments.next()?.with_default(0) {
                5 => terminal
                    .set_foreground_color(Color::Index(arguments.next()?.with_default(0) as u8)),
                2 => {
                    let r = arguments.next()?.with_default(0) as u8;
                    let g = arguments.next()?.with_default(0) as u8;
                    let b = arguments.next()?.with_default(0) as u8;
                    terminal.set_foreground_color(Color::Rgb([r, g, b]));
                }
                _ => return Err(ParseError::Invalid),
            },
            48 => match arguments.next()?.with_default(0) {
                5 => terminal
                    .set_background_color(Color::Index(arguments.next()?.with_default(0) as u8)),
                2 => {
                    let r = arguments.next()?.with_default(0) as u8;
                    let g = arguments.next()?.with_default(0) as u8;
                    let b = arguments.next()?.with_default(0) as u8;
                    terminal.set_background_color(Color::Rgb([r, g, b]));
                }
                _ => return Err(ParseError::Invalid),
            },

            _ => return Err(ParseError::Invalid),
        }

        if arguments.is_empty() {
            break;
        }
    }

    Ok(())
}

mod util {
    use super::{ByteIter, ParseError, ParseResult};

    pub fn take_while<'a>(
        iter: ByteIter<'_, 'a>,
        mut predicate: impl FnMut(u8) -> bool,
    ) -> ParseResult<&'a [u8]> {
        let bytes = iter.as_slice();

        let mut i = 0;
        loop {
            match bytes.get(i) {
                None => return Err(ParseError::Incomplete),
                Some(byte) if predicate(*byte) => i += 1,
                _ => break,
            }
        }

        let (matching, rest) = bytes.split_at(i);
        *iter = rest.iter();

        Ok(matching)
    }

    pub fn take_while_in_range<'a, R>(bytes: ByteIter<'_, 'a>, range: R) -> ParseResult<&'a [u8]>
    where
        R: std::ops::RangeBounds<u8>,
    {
        take_while(bytes, |byte| range.contains(&byte))
    }
}

#[derive(Debug, Copy, Clone, Default)]
pub struct Argument {
    value: Option<std::num::NonZeroU16>,
}

impl Argument {
    fn new(value: u16) -> Argument {
        Argument {
            value: std::num::NonZeroU16::new(value),
        }
    }

    pub fn with_default(self, default: u16) -> u16 {
        match self.value {
            None => default,
            Some(v) => v.get(),
        }
    }

    pub fn to_option(self) -> Option<u16> {
        self.value.map(|v| v.get())
    }

    pub fn single(parameters: &[u8]) -> ParseResult<Argument> {
        let mut value = 0u16;

        for byte in parameters {
            match byte {
                b'0'..=b'9' => {
                    let digit = byte - b'0';
                    value = value
                        .checked_mul(10)
                        .and_then(|v| v.checked_add(digit.into()))
                        .ok_or(ParseError::Invalid)?
                }
                _ => return Err(ParseError::Invalid),
            }
        }

        Ok(Argument::new(value))
    }

    pub fn iter(parameters: &[u8]) -> impl Iterator<Item = ParseResult<Argument>> + '_ {
        parameters
            .split(|byte| matches!(byte, b';' | b':'))
            .map(Self::single)
    }

    pub fn multi<const N: usize>(parameters: &[u8]) -> ParseResult<[Argument; N]> {
        let mut values = [Argument::default(); N];

        for (i, arg) in Self::iter(parameters).enumerate() {
            if i >= N {
                return Err(ParseError::Invalid);
            }

            values[i] = arg?;
        }

        Ok(values)
    }
}

pub struct ArgumentList<'a> {
    parameters: &'a [u8],
}

impl<'a> ArgumentList<'a> {
    pub fn new(parameters: &'a [u8]) -> ArgumentList {
        ArgumentList { parameters }
    }

    pub fn next(&mut self) -> ParseResult<Argument> {
        let separator = self
            .parameters
            .iter()
            .position(|byte| matches!(byte, b':' | b';'));

        match separator {
            Some(index) => {
                let (argument, rest) = self.parameters.split_at(index);
                self.parameters = &rest[1..];
                Argument::single(argument)
            }
            None => {
                let argument = self.parameters;
                self.parameters = &[];
                Argument::single(argument)
            }
        }
    }

    pub fn next_slice(&mut self) -> &'a [u8] {
        let separator = self
            .parameters
            .iter()
            .position(|byte| matches!(byte, b':' | b';'));

        match separator {
            Some(index) => {
                let (argument, rest) = self.parameters.split_at(index);
                self.parameters = &rest[1..];
                argument
            }
            None => {
                let argument = self.parameters;
                self.parameters = &[];
                argument
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.parameters.is_empty()
    }
}
