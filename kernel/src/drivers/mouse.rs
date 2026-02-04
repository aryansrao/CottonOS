//! PS/2 Mouse Driver

use spin::Mutex;

#[cfg(target_arch = "x86_64")]
use crate::arch::x86_64::{inb, outb};

#[cfg(target_arch = "aarch64")]
use crate::arch::aarch64::{inb, outb};

/// Mouse state
pub struct MouseState {
    pub x: i32,
    pub y: i32,
    pub buttons: u8,
    pub left: bool,
    pub right: bool,
    pub middle: bool,
    pub scroll_delta: i8,  // Scroll wheel delta
    screen_width: i32,
    screen_height: i32,
    cycle: u8,
    bytes: [u8; 4],
    has_scroll_wheel: bool,
}

impl MouseState {
    pub const fn new() -> Self {
        Self {
            x: 400,
            y: 300,
            buttons: 0,
            left: false,
            right: false,
            middle: false,
            scroll_delta: 0,
            screen_width: 800,
            screen_height: 600,
            cycle: 0,
            bytes: [0; 4],
            has_scroll_wheel: false,
        }
    }
    
    pub fn set_screen_size(&mut self, w: i32, h: i32) {
        self.screen_width = w;
        self.screen_height = h;
        self.x = w / 2;
        self.y = h / 2;
    }
    
    pub fn enable_scroll_wheel(&mut self) {
        self.has_scroll_wheel = true;
    }
    
    /// Process a byte from mouse
    pub fn process_byte(&mut self, byte: u8) {
        // For the first byte, check if it's valid (bit 3 must be set)
        // This helps resync if packets get out of order
        if self.cycle == 0 {
            if byte & 0x08 == 0 {
                // Invalid first byte, skip it
                return;
            }
        }
        
        self.bytes[self.cycle as usize] = byte;
        self.cycle += 1;
        
        let packet_size = if self.has_scroll_wheel { 4 } else { 3 };
        
        if self.cycle >= packet_size {
            self.cycle = 0;
            
            // We have a complete packet
            let flags = self.bytes[0];
            
            // Check for overflow - if set, discard packet
            if flags & 0xC0 != 0 {
                return;
            }
            
            self.buttons = flags & 0x07;
            self.left = flags & 0x01 != 0;
            self.right = flags & 0x02 != 0;
            self.middle = flags & 0x04 != 0;
            
            // X movement (signed)
            let mut dx = self.bytes[1] as i32;
            if flags & 0x10 != 0 {
                dx |= !0xFF; // Sign extend properly
            }
            
            // Y movement (signed)
            let mut dy = self.bytes[2] as i32;
            if flags & 0x20 != 0 {
                dy |= !0xFF; // Sign extend properly
            }
            
            // Scroll wheel (4th byte if enabled)
            if self.has_scroll_wheel {
                self.scroll_delta = self.bytes[3] as i8;
            } else {
                self.scroll_delta = 0;
            }
            
            // Update position (1:1 sensitivity)
            self.x += dx;
            self.y -= dy; // Y is inverted
            
            // Clamp to screen bounds
            if self.x < 0 { self.x = 0; }
            if self.y < 0 { self.y = 0; }
            if self.x >= self.screen_width { self.x = self.screen_width - 1; }
            if self.y >= self.screen_height { self.y = self.screen_height - 1; }
        }
    }
}

pub static MOUSE: Mutex<MouseState> = Mutex::new(MouseState::new());

/// Wait for mouse controller to be ready for input
fn mouse_wait_input() {
    for _ in 0..100000 {
        if inb(0x64) & 0x02 == 0 {
            return;
        }
    }
}

/// Wait for mouse data to be available
fn mouse_wait_output() {
    for _ in 0..100000 {
        if inb(0x64) & 0x01 != 0 {
            return;
        }
    }
}

/// Send command to mouse
fn mouse_write(cmd: u8) {
    mouse_wait_input();
    outb(0x64, 0xD4); // Tell controller we're sending to mouse
    mouse_wait_input();
    outb(0x60, cmd);
}

/// Read from mouse
fn mouse_read() -> u8 {
    mouse_wait_output();
    inb(0x60)
}

/// Initialize PS/2 mouse with scroll wheel support
pub fn init() {
    // Enable auxiliary device (mouse)
    mouse_wait_input();
    outb(0x64, 0xA8);
    
    // Enable interrupts
    mouse_wait_input();
    outb(0x64, 0x20); // Get compaq status
    mouse_wait_output();
    let status = inb(0x60);
    let status = status | 0x02; // Enable IRQ12
    mouse_wait_input();
    outb(0x64, 0x60); // Set compaq status
    mouse_wait_input();
    outb(0x60, status);
    
    // Use default settings
    mouse_write(0xF6);
    mouse_read(); // ACK
    
    // Try to enable scroll wheel (IntelliMouse protocol)
    // Magic sequence: set sample rate to 200, 100, 80
    mouse_write(0xF3); mouse_read(); // Set sample rate
    mouse_write(200); mouse_read();
    mouse_write(0xF3); mouse_read();
    mouse_write(100); mouse_read();
    mouse_write(0xF3); mouse_read();
    mouse_write(80); mouse_read();
    
    // Get device ID to check if scroll wheel is enabled
    mouse_write(0xF2); // Get device ID
    mouse_read(); // ACK
    let id = mouse_read();
    
    if id == 3 || id == 4 {
        // IntelliMouse with scroll wheel detected
        MOUSE.lock().enable_scroll_wheel();
        crate::kprintln!("[MOUSE] PS/2 mouse initialized with scroll wheel");
    } else {
        crate::kprintln!("[MOUSE] PS/2 mouse initialized (no scroll wheel)");
    }
    
    // Enable mouse
    mouse_write(0xF4);
    mouse_read(); // ACK
}

/// Handle mouse interrupt (IRQ12)
pub fn handle_interrupt() {
    let byte = inb(0x60);
    let mut mouse = MOUSE.lock();
    mouse.process_byte(byte);
}

/// Get current mouse position
pub fn get_position() -> (i32, i32) {
    let mouse = MOUSE.lock();
    (mouse.x, mouse.y)
}

/// Get scroll wheel delta and clear it
pub fn get_scroll_delta() -> i8 {
    let mut mouse = MOUSE.lock();
    let delta = mouse.scroll_delta;
    mouse.scroll_delta = 0;
    delta
}

/// Get mouse button state
pub fn get_buttons() -> (bool, bool, bool) {
    let mouse = MOUSE.lock();
    (mouse.left, mouse.right, mouse.middle)
}

/// Check if left button is pressed
pub fn left_pressed() -> bool {
    MOUSE.lock().left
}

/// Check if right button is pressed
pub fn right_pressed() -> bool {
    MOUSE.lock().right
}
