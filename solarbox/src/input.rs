//! Per-frame keyboard input handed from the window to the console shell.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EKey {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Backspace,
    Delete,
    Enter,
    Tab,
}

#[derive(Clone, Default)]
pub struct FrameInput {
    /// Escape / F5 pressed this frame (edge-triggered).
    pub esc: bool,
    pub reload: bool,
    /// Printable characters typed this frame (layout-aware).
    pub chars: Vec<char>,
    /// Navigation/editing keys, with key repeat.
    pub keys: Vec<EKey>,
    /// Letters pressed while Ctrl was held (lowercase), e.g. 's' for Ctrl+S.
    pub shortcuts: Vec<char>,
    pub ctrl: bool,
    pub shift: bool,
}
