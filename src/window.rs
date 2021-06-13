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
    Char(char),
    KeyPress(Key),
    ScaleFactorChanged,
    EventsCleared,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    Enter,
    Backspace,
    Tab,
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
