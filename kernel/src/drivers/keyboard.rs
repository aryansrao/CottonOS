//! Keyboard Driver
//!
//! PS/2 keyboard driver for x86, GPIO keyboard for ARM

use spin::Mutex;
use alloc::collections::VecDeque;

/// Keyboard buffer
static KEYBOARD_BUFFER: Mutex<VecDeque<KeyEvent>> = Mutex::new(VecDeque::new());

/// Track if we're in an extended scancode sequence
static EXTENDED_KEY: Mutex<bool> = Mutex::new(false);

/// Key event
#[derive(Clone, Copy, Debug)]
pub struct KeyEvent {
    pub scancode: u8,
    pub keycode: KeyCode,
    pub modifiers: Modifiers,
    pub pressed: bool,
}

/// Key code enumeration
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum KeyCode {
    Unknown,
    
    // Letters
    A, B, C, D, E, F, G, H, I, J, K, L, M,
    N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    
    // Numbers
    Key0, Key1, Key2, Key3, Key4, Key5, Key6, Key7, Key8, Key9,
    
    // Function keys
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
    
    // Special keys
    Escape,
    Tab,
    CapsLock,
    LeftShift,
    RightShift,
    LeftCtrl,
    RightCtrl,
    LeftAlt,
    RightAlt,
    Space,
    Enter,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    
    // Arrow keys
    Up, Down, Left, Right,
    
    // Punctuation
    Minus,
    Equals,
    LeftBracket,
    RightBracket,
    Backslash,
    Semicolon,
    Quote,
    Grave,
    Comma,
    Period,
    Slash,
    
    // Keypad
    NumLock,
    ScrollLock,
    Keypad0, Keypad1, Keypad2, Keypad3, Keypad4,
    Keypad5, Keypad6, Keypad7, Keypad8, Keypad9,
    KeypadPlus,
    KeypadMinus,
    KeypadMultiply,
    KeypadDivide,
    KeypadEnter,
    KeypadPeriod,
}

/// Modifier keys
#[derive(Clone, Copy, Debug, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub caps_lock: bool,
    pub num_lock: bool,
}

/// Current modifiers state
static MODIFIERS: Mutex<Modifiers> = Mutex::new(Modifiers {
    shift: false,
    ctrl: false,
    alt: false,
    caps_lock: false,
    num_lock: false,
});

/// Initialize keyboard
pub fn init() {
    #[cfg(target_arch = "x86_64")]
    init_ps2();
    
    crate::kprintln!("[KEYBOARD] Keyboard initialized");
}

#[cfg(target_arch = "x86_64")]
fn init_ps2() {
    use crate::arch::x86_64::{inb, outb};
    
    // Wait for keyboard controller
    while inb(0x64) & 0x02 != 0 {}
    
    // Disable devices
    outb(0x64, 0xAD);
    outb(0x64, 0xA7);
    
    // Flush output buffer
    while inb(0x64) & 0x01 != 0 {
        inb(0x60);
    }
    
    // Read configuration
    outb(0x64, 0x20);
    while inb(0x64) & 0x01 == 0 {}
    let config = inb(0x60);
    
    // Enable IRQ1, ENABLE translation (bit 6) to get scancode set 1
    // Translation converts scancode set 2 to set 1
    let config = (config | 0x01) | 0x40;
    
    // Write configuration
    outb(0x64, 0x60);
    while inb(0x64) & 0x02 != 0 {}
    outb(0x60, config);
    
    // Enable first port
    outb(0x64, 0xAE);
    
    // Reset keyboard and wait for ACK
    while inb(0x64) & 0x02 != 0 {}
    outb(0x60, 0xFF);
    
    // Wait for keyboard response (0xFA = ACK, 0xAA = self-test passed)
    // Give it some time to respond
    for _ in 0..10000 {
        if inb(0x64) & 0x01 != 0 {
            let _ = inb(0x60);  // Read and discard response
        }
    }
}

/// Handle keyboard interrupt
#[cfg(target_arch = "x86_64")]
pub fn handle_interrupt() {
    use crate::arch::x86_64::inb;
    
    let scancode = inb(0x60);
    
    // Check for extended scancode prefix
    if scancode == 0xE0 {
        *EXTENDED_KEY.lock() = true;
        return;
    }
    
    let is_extended = {
        let mut ext = EXTENDED_KEY.lock();
        let was_extended = *ext;
        *ext = false;
        was_extended
    };
    
    if let Some(event) = process_scancode(scancode, is_extended) {
        let mut buffer = KEYBOARD_BUFFER.lock();
        if buffer.len() < 256 {
            buffer.push_back(event);
        }
    }
}

/// Process PS/2 scancode (set 1)
fn process_scancode(scancode: u8, extended: bool) -> Option<KeyEvent> {
    let pressed = scancode & 0x80 == 0;
    let code = scancode & 0x7F;
    
    let keycode = if extended {
        extended_scancode_to_keycode(code)
    } else {
        scancode_to_keycode(code)
    };
    
    // Update modifiers
    {
        let mut mods = MODIFIERS.lock();
        match keycode {
            KeyCode::LeftShift | KeyCode::RightShift => mods.shift = pressed,
            KeyCode::LeftCtrl | KeyCode::RightCtrl => mods.ctrl = pressed,
            KeyCode::LeftAlt | KeyCode::RightAlt => mods.alt = pressed,
            KeyCode::CapsLock if pressed => mods.caps_lock = !mods.caps_lock,
            KeyCode::NumLock if pressed => mods.num_lock = !mods.num_lock,
            _ => {}
        }
    }
    
    let modifiers = *MODIFIERS.lock();
    
    Some(KeyEvent {
        scancode,
        keycode,
        modifiers,
        pressed,
    })
}

/// Convert extended scancode (after 0xE0) to keycode
fn extended_scancode_to_keycode(scancode: u8) -> KeyCode {
    match scancode {
        0x1C => KeyCode::KeypadEnter,
        0x1D => KeyCode::RightCtrl,
        0x35 => KeyCode::KeypadDivide,
        0x38 => KeyCode::RightAlt,
        0x47 => KeyCode::Home,
        0x48 => KeyCode::Up,
        0x49 => KeyCode::PageUp,
        0x4B => KeyCode::Left,
        0x4D => KeyCode::Right,
        0x4F => KeyCode::End,
        0x50 => KeyCode::Down,
        0x51 => KeyCode::PageDown,
        0x52 => KeyCode::Insert,
        0x53 => KeyCode::Delete,
        _ => KeyCode::Unknown,
    }
}

/// Convert scancode to keycode
fn scancode_to_keycode(scancode: u8) -> KeyCode {
    match scancode {
        0x01 => KeyCode::Escape,
        0x02 => KeyCode::Key1,
        0x03 => KeyCode::Key2,
        0x04 => KeyCode::Key3,
        0x05 => KeyCode::Key4,
        0x06 => KeyCode::Key5,
        0x07 => KeyCode::Key6,
        0x08 => KeyCode::Key7,
        0x09 => KeyCode::Key8,
        0x0A => KeyCode::Key9,
        0x0B => KeyCode::Key0,
        0x0C => KeyCode::Minus,
        0x0D => KeyCode::Equals,
        0x0E => KeyCode::Backspace,
        0x0F => KeyCode::Tab,
        0x10 => KeyCode::Q,
        0x11 => KeyCode::W,
        0x12 => KeyCode::E,
        0x13 => KeyCode::R,
        0x14 => KeyCode::T,
        0x15 => KeyCode::Y,
        0x16 => KeyCode::U,
        0x17 => KeyCode::I,
        0x18 => KeyCode::O,
        0x19 => KeyCode::P,
        0x1A => KeyCode::LeftBracket,
        0x1B => KeyCode::RightBracket,
        0x1C => KeyCode::Enter,
        0x1D => KeyCode::LeftCtrl,
        0x1E => KeyCode::A,
        0x1F => KeyCode::S,
        0x20 => KeyCode::D,
        0x21 => KeyCode::F,
        0x22 => KeyCode::G,
        0x23 => KeyCode::H,
        0x24 => KeyCode::J,
        0x25 => KeyCode::K,
        0x26 => KeyCode::L,
        0x27 => KeyCode::Semicolon,
        0x28 => KeyCode::Quote,
        0x29 => KeyCode::Grave,
        0x2A => KeyCode::LeftShift,
        0x2B => KeyCode::Backslash,
        0x2C => KeyCode::Z,
        0x2D => KeyCode::X,
        0x2E => KeyCode::C,
        0x2F => KeyCode::V,
        0x30 => KeyCode::B,
        0x31 => KeyCode::N,
        0x32 => KeyCode::M,
        0x33 => KeyCode::Comma,
        0x34 => KeyCode::Period,
        0x35 => KeyCode::Slash,
        0x36 => KeyCode::RightShift,
        0x37 => KeyCode::KeypadMultiply,
        0x38 => KeyCode::LeftAlt,
        0x39 => KeyCode::Space,
        0x3A => KeyCode::CapsLock,
        0x3B => KeyCode::F1,
        0x3C => KeyCode::F2,
        0x3D => KeyCode::F3,
        0x3E => KeyCode::F4,
        0x3F => KeyCode::F5,
        0x40 => KeyCode::F6,
        0x41 => KeyCode::F7,
        0x42 => KeyCode::F8,
        0x43 => KeyCode::F9,
        0x44 => KeyCode::F10,
        0x45 => KeyCode::NumLock,
        0x46 => KeyCode::ScrollLock,
        0x47 => KeyCode::Keypad7,
        0x48 => KeyCode::Keypad8,
        0x49 => KeyCode::Keypad9,
        0x4A => KeyCode::KeypadMinus,
        0x4B => KeyCode::Keypad4,
        0x4C => KeyCode::Keypad5,
        0x4D => KeyCode::Keypad6,
        0x4E => KeyCode::KeypadPlus,
        0x4F => KeyCode::Keypad1,
        0x50 => KeyCode::Keypad2,
        0x51 => KeyCode::Keypad3,
        0x52 => KeyCode::Keypad0,
        0x53 => KeyCode::KeypadPeriod,
        0x57 => KeyCode::F11,
        0x58 => KeyCode::F12,
        _ => KeyCode::Unknown,
    }
}

/// Convert key event to character
pub fn keyevent_to_char(event: &KeyEvent) -> Option<char> {
    if !event.pressed {
        return None;
    }
    
    let shift = event.modifiers.shift ^ event.modifiers.caps_lock;
    
    let c = match event.keycode {
        KeyCode::A => if shift { 'A' } else { 'a' },
        KeyCode::B => if shift { 'B' } else { 'b' },
        KeyCode::C => if shift { 'C' } else { 'c' },
        KeyCode::D => if shift { 'D' } else { 'd' },
        KeyCode::E => if shift { 'E' } else { 'e' },
        KeyCode::F => if shift { 'F' } else { 'f' },
        KeyCode::G => if shift { 'G' } else { 'g' },
        KeyCode::H => if shift { 'H' } else { 'h' },
        KeyCode::I => if shift { 'I' } else { 'i' },
        KeyCode::J => if shift { 'J' } else { 'j' },
        KeyCode::K => if shift { 'K' } else { 'k' },
        KeyCode::L => if shift { 'L' } else { 'l' },
        KeyCode::M => if shift { 'M' } else { 'm' },
        KeyCode::N => if shift { 'N' } else { 'n' },
        KeyCode::O => if shift { 'O' } else { 'o' },
        KeyCode::P => if shift { 'P' } else { 'p' },
        KeyCode::Q => if shift { 'Q' } else { 'q' },
        KeyCode::R => if shift { 'R' } else { 'r' },
        KeyCode::S => if shift { 'S' } else { 's' },
        KeyCode::T => if shift { 'T' } else { 't' },
        KeyCode::U => if shift { 'U' } else { 'u' },
        KeyCode::V => if shift { 'V' } else { 'v' },
        KeyCode::W => if shift { 'W' } else { 'w' },
        KeyCode::X => if shift { 'X' } else { 'x' },
        KeyCode::Y => if shift { 'Y' } else { 'y' },
        KeyCode::Z => if shift { 'Z' } else { 'z' },
        
        KeyCode::Key0 => if event.modifiers.shift { ')' } else { '0' },
        KeyCode::Key1 => if event.modifiers.shift { '!' } else { '1' },
        KeyCode::Key2 => if event.modifiers.shift { '@' } else { '2' },
        KeyCode::Key3 => if event.modifiers.shift { '#' } else { '3' },
        KeyCode::Key4 => if event.modifiers.shift { '$' } else { '4' },
        KeyCode::Key5 => if event.modifiers.shift { '%' } else { '5' },
        KeyCode::Key6 => if event.modifiers.shift { '^' } else { '6' },
        KeyCode::Key7 => if event.modifiers.shift { '&' } else { '7' },
        KeyCode::Key8 => if event.modifiers.shift { '*' } else { '8' },
        KeyCode::Key9 => if event.modifiers.shift { '(' } else { '9' },
        
        KeyCode::Space => ' ',
        KeyCode::Enter => '\n',
        KeyCode::Tab => '\t',
        KeyCode::Backspace => '\x08',
        KeyCode::Escape => '\x1b',
        KeyCode::Delete => '\x7f',
        
        KeyCode::Minus => if event.modifiers.shift { '_' } else { '-' },
        KeyCode::Equals => if event.modifiers.shift { '+' } else { '=' },
        KeyCode::LeftBracket => if event.modifiers.shift { '{' } else { '[' },
        KeyCode::RightBracket => if event.modifiers.shift { '}' } else { ']' },
        KeyCode::Backslash => if event.modifiers.shift { '|' } else { '\\' },
        KeyCode::Semicolon => if event.modifiers.shift { ':' } else { ';' },
        KeyCode::Quote => if event.modifiers.shift { '"' } else { '\'' },
        KeyCode::Grave => if event.modifiers.shift { '~' } else { '`' },
        KeyCode::Comma => if event.modifiers.shift { '<' } else { ',' },
        KeyCode::Period => if event.modifiers.shift { '>' } else { '.' },
        KeyCode::Slash => if event.modifiers.shift { '?' } else { '/' },
        
        KeyCode::Keypad0 => '0',
        KeyCode::Keypad1 => '1',
        KeyCode::Keypad2 => '2',
        KeyCode::Keypad3 => '3',
        KeyCode::Keypad4 => '4',
        KeyCode::Keypad5 => '5',
        KeyCode::Keypad6 => '6',
        KeyCode::Keypad7 => '7',
        KeyCode::Keypad8 => '8',
        KeyCode::Keypad9 => '9',
        KeyCode::KeypadPlus => '+',
        KeyCode::KeypadMinus => '-',
        KeyCode::KeypadMultiply => '*',
        KeyCode::KeypadDivide => '/',
        KeyCode::KeypadEnter => '\n',
        KeyCode::KeypadPeriod => '.',
        
        _ => return None,
    };
    
    Some(c)
}

/// Read key event from buffer
pub fn read_key() -> Option<KeyEvent> {
    KEYBOARD_BUFFER.lock().pop_front()
}

/// Read character from keyboard (blocking)
pub fn read_char() -> Option<char> {
    if let Some(event) = read_key() {
        keyevent_to_char(&event)
    } else {
        None
    }
}

/// Get next printable character, skipping non-printable events
pub fn get_char() -> Option<char> {
    // Keep reading events until we get a printable character or buffer is empty
    while let Some(event) = read_key() {
        if let Some(c) = keyevent_to_char(&event) {
            return Some(c);
        }
        // If no char (e.g., key release or modifier), continue checking buffer
    }
    None
}

/// Check if keyboard buffer has data
pub fn has_key() -> bool {
    !KEYBOARD_BUFFER.lock().is_empty()
}
