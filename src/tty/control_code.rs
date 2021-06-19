pub trait Terminal {
    fn invalid_control_sequence(&mut self, bytes: &[u8]);

    /// Output some text to the screen
    fn text(&mut self, text: &str);

    /// Encountered invalid UTF-8
    fn invalid_utf8(&mut self, text: &[u8]);

    /// Makes an audible bell
    fn bell(&mut self);

    // === CURSOR MOVEMENT === //

    /// Move cursor to next column that is an multiple of 8
    fn tab(&mut self);

    /// Move cursor to the left, might wrap to previous line
    fn backspace(&mut self);

    /// `\r`
    fn carriage_return(&mut self);
    /// `\n`
    fn line_feed(&mut self);

    /// Move the cursor in the given direction
    fn move_cursor(&mut self, direction: Direction, steps: u16);

    /// Sets the position of the cursor relative to the top-left corner (0-indexed)
    fn set_cursor_pos(&mut self, row: u16, col: u16);

    // === CLEARING === //

    /// Clear from cursor to the end of the line
    fn clear_line(&mut self, region: ClearRegion);

    /// Clear entire screen and scrollback buffer
    fn clear_screen(&mut self, region: ClearRegion);

    /// Clear scrollback buffer
    fn clear_scrollback(&mut self);

    // === BEHAVIOUR === //

    /// If enabled: surround text pasted into terminal with `ESC [200~` and `ESC [201~`
    fn set_bracketed_paste(&mut self, toggle: Toggle);

    /// If enabled: arrow keys should send application codes instead of ANSI codes
    fn set_application_cursor(&mut self, toggle: Toggle);

    // === CHARACTER STYLE === //

    /// Set the style of characters
    fn set_character_style(&mut self, style: CharacterStyles);

    /// Set the style of characters
    fn reset_character_style(&mut self, style: CharacterStyles);

    // === COLOR === //

    /// Set the color of the foreground
    fn set_foreground_color(&mut self, color: Color);

    /// Reset the foreground to the default color
    fn reset_foreground_color(&mut self);

    /// Set the color of the background
    fn set_background_color(&mut self, color: Color);

    /// Reset the background to the default color
    fn reset_background_color(&mut self);

    // === COLOR === //

    /// Set the title of the window
    fn set_window_title(&mut self, text: &str);
}

pub trait Window {}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Toggle {
    Enable,
    Disable,
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

#[derive(Debug, Clone)]
pub enum Color {
    /// Use a color from the default palette
    Index(u8),
    /// Use a specific RGB color
    Rgb([u8; 3]),
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

struct ControlSequenceArguments {
    values: [u16; 5],
    count: u8,
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

        _ => return Err(ParseError::Invalid),
    }

    Ok(())
}

fn parse_operating_system_command(
    bytes: ByteIter,
    terminal: &mut impl Terminal,
) -> ParseResult<()> {
    let numbers = util::take_while_in_range(bytes, b'0'..=b'9')?;
    let parameters = util::take_while_in_range(bytes, 0x20..=0x7e)?;
    let terminator = bytes.next().ok_or(ParseError::Incomplete)?;

    if !matches!(terminator, b'\x07') {
        return Err(ParseError::Invalid);
    }

    let parameters = parameters.strip_prefix(b";").ok_or(ParseError::Invalid)?;

    if parameters == b"?" {
        // TODO: this should instead respond with a `TerminalCode::GetWindowTitle` or similar
        return Err(ParseError::Invalid);
    }

    match Argument::single(numbers)?.with_default(0) {
        // Change "icon name" and window title. The former does not apply.
        0 => {
            let text = std::str::from_utf8(parameters).unwrap();
            terminal.set_window_title(text);
        }

        // Change "icon name" (does not apply)
        1 => {}

        // Change window title.
        2 => {
            let text = std::str::from_utf8(parameters).unwrap();
            terminal.set_window_title(text);
        }

        // Set X-property on top-level window (does not apply)
        3 => {}

        _ => return Err(ParseError::Invalid),
    }

    Ok(())
}

fn parse_escape_control_sequence(bytes: ByteIter, terminal: &mut impl Terminal) -> ParseResult<()> {
    let (parameters, intermediate, terminator) = parse_control_sequence_parts(bytes)?;

    // TODO: figure out when to use intermediate bytes
    if !intermediate.is_empty() {
        return Err(ParseError::Invalid);
    }

    match parameters {
        [b'?', arguments @ ..] => parse_escape_question_terminator(arguments, terminator, terminal),
        arguments => parse_escape_standard_terminator(arguments, terminator, terminal),
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
        b'h' => parse_question_terminator_toggle(parameters, Toggle::Enable, terminal),
        b'l' => parse_question_terminator_toggle(parameters, Toggle::Disable, terminal),
        _ => Err(ParseError::Invalid),
    }
}

fn parse_question_terminator_toggle(
    parameters: &[u8],
    toggle: Toggle,
    terminal: &mut impl Terminal,
) -> ParseResult<()> {
    match Argument::single(parameters)?.with_default(0) {
        1 => terminal.set_application_cursor(toggle),
        2004 => terminal.set_bracketed_paste(toggle),
        _ => return Err(ParseError::Invalid),
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

        _ => return Err(ParseError::Invalid),
    }

    Ok(())
}

fn parse_character_attribute(parameters: &[u8], terminal: &mut impl Terminal) -> ParseResult<()> {
    for group in parameters.split(|byte| *byte == b';') {
        let mut arguments = Argument::list(group);

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

            arg @ 30..=37 => terminal.set_foreground_color(Color::Index(arg as u8 - 30)),
            arg @ 90..=97 => terminal.set_foreground_color(Color::Index(arg as u8 - 90)),
            39 => terminal.reset_foreground_color(),

            arg @ 40..=47 => terminal.set_background_color(Color::Index(arg as u8 - 30)),
            arg @ 100..=107 => terminal.set_background_color(Color::Index(arg as u8 - 100)),
            49 => terminal.reset_background_color(),

            _ => return Err(ParseError::Invalid),
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

/*
pub struct StreamingParser {}

impl StreamingParser {
    pub fn new() -> StreamingParser {
        StreamingParser {}
    }

    pub fn parse_terminal_command<'a>(
        &mut self,
        text: &'a str,
        mut emit: impl FnMut(TerminalCommand),
    ) -> &'a str {
        let mut chars = text.chars();

        let mut unprocessed = chars.as_str();

        loop {
            match chars.next() {
                None => {
                    if !unprocessed.is_empty() {
                        emit(BufferCommand::Text(unprocessed.into()).into());
                    }
                    return "";
                }
                Some(ch) if ch.is_ascii_control() => {
                    let remaining_bytes = 1 + chars.as_str().len();
                    let text_len = unprocessed.len() - remaining_bytes;
                    if text_len > 0 {
                        let (text, rest) = unprocessed.split_at(text_len);
                        emit(BufferCommand::Text(text.into()).into());
                        unprocessed = rest;
                    }

                    let parse_result = match ch {
                        '\x07' => Ok(BufferCommand::Bell.into()),
                        '\x08' => Ok(BufferCommand::Backspace.into()),
                        '\x09' => Ok(BufferCommand::Tab.into()),
                        '\r' => Ok(BufferCommand::CarriageReturn.into()),
                        '\n' => Ok(BufferCommand::LineFeed.into()),
                        '\x1b' => match chars.next() {
                            // Control Sequence
                            Some('[') => self
                                .parse_control_sequence(&mut chars)
                                .map(TerminalCommand::from),

                            // Operating System Command
                            Some(']') => self
                                .parse_operating_system_command(&mut chars)
                                .map(TerminalCommand::from),

                            // sets the character set: https://invisible-island.net/xterm/ctlseqs/ctlseqs.html
                            Some('(') => match chars.next() {
                                None => Err(ParseError::Incomplete),
                                Some(_) => Err(ParseError::Ignored),
                            },

                            // Enter alternate keypad mode
                            Some('=') => Err(ParseError::Ignored),

                            // Exit alternate keypad mode
                            Some('>') => Err(ParseError::Ignored),

                            Some(_) => Err(ParseError::Invalid),
                            None => Err(ParseError::Incomplete),
                        },
                        _ => Err(ParseError::Invalid),
                    };

                    match parse_result {
                        Ok(code) => emit(code),
                        Err(ParseError::Incomplete) => return unprocessed,
                        Err(ParseError::Invalid) => {
                            let len = chars.as_str().len();
                            let sequence = &unprocessed[..unprocessed.len() - len];
                            emit(TerminalCommand::Unknown(sequence.into()));
                        }
                        Err(ParseError::Ignored) => {
                            let len = chars.as_str().len();
                            let sequence = &unprocessed[..unprocessed.len() - len];
                            emit(TerminalCommand::Ignored(sequence.into()));
                        }
                    }

                    unprocessed = chars.as_str();
                }
                _ => continue,
            }
        }
    }

    fn parse_control_sequence_parts<'a>(
        chars: &mut std::str::Chars<'a>,
    ) -> ParseResult<(&'a str, &'a str, u8)> {
        let parameters = take_while_in_range(chars, 0x30..=0x3f)?;
        let intermediate = take_while_in_range(chars, 0x20..=0x2f)?;
        let terminator = chars.next().ok_or(ParseError::Incomplete)?;

        if ('\x40'..='\x7f').contains(&terminator) {
            Ok((parameters, intermediate, terminator as u8))
        } else {
            Err(ParseError::Invalid)
        }
    }

    fn parse_control_sequence(
        &mut self,
        chars: &mut std::str::Chars,
    ) -> ParseResult<BufferCommand> {
        let (parameters, intermediate, terminator) = Self::parse_control_sequence_parts(chars)?;

        // TODO: figure out how to handle intermediate bytes
        if !intermediate.is_empty() {
            return Err(ParseError::Invalid);
        }

        let parameters = parameters.as_bytes();

        match parameters {
            [b'?', parameters @ ..] => Self::parse_question_terminator(parameters, terminator),
            _ => Self::parse_standard_terminator(parameters, terminator),
        }
    }

    fn parse_question_terminator(parameters: &[u8], terminator: u8) -> ParseResult<BufferCommand> {
        let arguments = ControlSequenceArguments::parse(parameters)?;

        match (arguments.as_slice(), terminator) {
            ([1], b'h') => Ok(BufferCommand::SetApplicationCursor(true)),
            ([1], b'l') => Ok(BufferCommand::SetApplicationCursor(false)),

            ([2004], b'h') => Ok(BufferCommand::SetBracketedPaste(true)),
            ([2004], b'l') => Ok(BufferCommand::SetBracketedPaste(false)),

            _ => Err(ParseError::Invalid),
        }
    }

    fn parse_standard_terminator(parameters: &[u8], terminator: u8) -> ParseResult<BufferCommand> {
        let move_cursor = |direction| -> ParseResult<_> {
            let steps = arg::single(parameters)?.unwrap_or(1);
            Ok(BufferCommand::MoveCursor { direction, steps })
        };

        match terminator {
            b'A' => move_cursor(Direction::Up),
            b'B' => move_cursor(Direction::Down),
            b'C' => move_cursor(Direction::Right),
            b'D' => move_cursor(Direction::Left),

            b'H' => {
                let [row, col] = arg::multi(parameters, [1, 1])?;
                Ok(BufferCommand::SetCursorPos([row - 1, col - 1]))
            }

            b'X' => Ok(BufferCommand::Erase {
                count: arg::single(parameters)?.unwrap_or(1),
            }),

            b'J' => match arg::single(parameters)?.unwrap_or(0) {
                0 => Ok(BufferCommand::ClearScreenToEnd),
                1 => Ok(BufferCommand::ClearScreenToStart),
                2 => Ok(BufferCommand::ClearScreen),
                3 => Ok(BufferCommand::ClearScreenAndScrollback),
                _ => Err(ParseError::Invalid),
            },

            b'K' => match arg::single(parameters)?.unwrap_or(0) {
                0 => Ok(BufferCommand::ClearLineToEnd),
                1 => Ok(BufferCommand::ClearLineToStart),
                2 => Ok(BufferCommand::ClearLine),
                _ => Err(ParseError::Invalid),
            },

            b'm' => Self::parse_character_attribute(parameters),

            _ => Err(ParseError::Invalid),
        }
    }

    fn parse_character_attribute(parameters: &[u8]) -> ParseResult<BufferCommand> {
        use CharacterAttributes::{
            ResetBackground, ResetForeground, ResetStyles, SetBackground, SetForeground, SetStyles,
        };

        let mut attributes = Vec::new();

        for group in parameters.split(|byte| *byte == b';') {
            let mut arguments = arg::iter(group);
            let kind = arguments.next().unwrap_or(Ok(None))?.unwrap_or(0);

            let attribute = match kind {
                0 => CharacterAttributes::ResetAll,

                1 => SetStyles(CharacterStyles::BOLD),
                21 => ResetStyles(CharacterStyles::BOLD),

                2 => SetStyles(CharacterStyles::FAINT),
                22 => ResetStyles(CharacterStyles::FAINT),

                3 => SetStyles(CharacterStyles::ITALIC),
                23 => ResetStyles(CharacterStyles::ITALIC),

                4 => SetStyles(CharacterStyles::UNDERLINE),
                24 => ResetStyles(CharacterStyles::UNDERLINE),

                5 => SetStyles(CharacterStyles::BLINK),
                25 => ResetStyles(CharacterStyles::BLINK),

                7 => SetStyles(CharacterStyles::INVERSE),
                27 => ResetStyles(CharacterStyles::INVERSE),

                8 => SetStyles(CharacterStyles::INVISIBLE),
                28 => ResetStyles(CharacterStyles::INVISIBLE),

                9 => SetStyles(CharacterStyles::STRIKETHROUGH),
                29 => ResetStyles(CharacterStyles::STRIKETHROUGH),

                arg @ 30..=37 => SetForeground(Color::Index(arg as u8 - 30)),
                arg @ 90..=97 => SetForeground(Color::Index(arg as u8 - 90)),
                39 => ResetForeground,

                arg @ 40..=47 => SetBackground(Color::Index(arg as u8 - 30)),
                arg @ 100..=107 => SetForeground(Color::Index(arg as u8 - 100)),
                49 => ResetBackground,

                _ => return Err(ParseError::Invalid),
            };

            attributes.push(attribute);

            if arguments.next().is_some() {
                // unexpected argument
                return Err(ParseError::Invalid);
            }
        }

        Ok(BufferCommand::CharacterAttributes(
            attributes.into_boxed_slice(),
        ))
    }

    fn parse_operating_system_command(
        &self,
        chars: &mut std::str::Chars,
    ) -> ParseResult<OsCommand> {
        let numbers = take_while_in_range(chars, b'0'..=b'9')?;
        let parameters = take_while_in_range(chars, 0x20..=0x7e)?;
        let terminator = chars.next().ok_or(ParseError::Incomplete)?;

        if !matches!(terminator, '\x07') {
            return Err(ParseError::Invalid);
        }

        let parameters = parameters.strip_prefix(';').ok_or(ParseError::Invalid)?;

        if parameters == "?" {
            // TODO: this should instead respond with a `TerminalCode::GetWindowTitle` or similar
            return Err(ParseError::Invalid);
        }

        match numbers {
            // Change "icon name" and window title. The former does not apply.
            "0" => Ok(OsCommand::SetWindowTitle(parameters.into())),

            // Change "icon name" (does not apply)
            "1" => Err(ParseError::Ignored),

            // Change window title.
            "2" => Ok(OsCommand::SetWindowTitle(parameters.into())),

            // Set X-property on top-level window (does not apply)
            "3" => Err(ParseError::Ignored),

            _ => Err(ParseError::Invalid),
        }
    }
}

impl ControlSequenceArguments {
    pub fn parse(bytes: &[u8]) -> ParseResult<ControlSequenceArguments> {
        let mut values = [0u16; 5];
        let mut index = 0;

        for &byte in bytes {
            match byte {
                b'0'..=b'9' => {
                    let digit = u16::from(byte - b'0');
                    values[index] = values[index]
                        .checked_mul(10)
                        .and_then(|value| value.checked_add(digit))
                        .ok_or(ParseError::Invalid)?;
                }
                b':' | b';' => {
                    index += 1;
                    if index >= values.len() {
                        return Err(ParseError::Invalid);
                    }
                }

                // reserved for private use
                _ => return Err(ParseError::Invalid),
            }
        }

        Ok(ControlSequenceArguments {
            values,
            count: index as u8 + 1,
        })
    }

    pub fn get_default(&self, index: usize, default: u16) -> u16 {
        match self.values[index] {
            0 => default,
            value => value,
        }
    }

    pub fn get(&self, index: usize) -> u16 {
        self.values[index]
    }

    pub fn as_slice(&self) -> &[u16] {
        &self.values[..self.count as usize]
    }
}
*/

#[derive(Debug, Copy, Clone, Default)]
pub struct Argument {
    value: Option<std::num::NonZeroU16>,
}

pub struct ArgumentList<'a> {
    parameters: &'a [u8],
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

    pub fn list(parameters: &[u8]) -> ArgumentList {
        ArgumentList { parameters }
    }
}

impl ArgumentList<'_> {
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
}
