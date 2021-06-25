#[cfg(target_os = "macos")]
pub mod cocoa;
#[cfg(target_os = "macos")]
pub use self::cocoa as platform;

pub use platform::{EventLoop, EventLoopWaker, Window};

#[derive(Debug)]
pub struct WindowConfig {
    pub size: PhysicalSize,
}

#[derive(Debug)]
pub enum Event {
    Active,
    Inactive,
    Resize(PhysicalSize),
    KeyPress(Key, Modifiers),
    ScaleFactorChanged,
    EventsCleared,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    Char(char),
    Escape,
    Enter,
    Backspace,
    Tab,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
}

bitflags::bitflags! {
    pub struct Modifiers: u8 {
        const CONTROL = 1;
        const SHIFT = 2;
        const ALT = 4;
        const SUPER = 8;
    }
}

impl Modifiers {
    pub const EMPTY: Modifiers = Modifiers::empty();
}

/// A size in physical pixels
#[derive(Debug, Copy, Clone)]
pub struct PhysicalSize {
    pub width: u32,
    pub height: u32,
}

impl PhysicalSize {
    pub fn new(width: u32, height: u32) -> PhysicalSize {
        PhysicalSize { width, height }
    }
}
