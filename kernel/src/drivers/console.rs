//! Console Driver

use spin::Mutex;

/// Console writer
pub static CONSOLE: Mutex<Console> = Mutex::new(Console::new());

/// Console state
pub struct Console {
    #[cfg(target_arch = "x86_64")]
    pub col: usize,
    #[cfg(target_arch = "x86_64")]
    pub row: usize,
    #[cfg(target_arch = "x86_64")]
    pub color: u8,
    
    #[cfg(target_arch = "aarch64")]
    pub x: usize,
    #[cfg(target_arch = "aarch64")]
    pub y: usize,
}

impl Console {
    pub const fn new() -> Self {
        Self {
            #[cfg(target_arch = "x86_64")]
            col: 0,
            #[cfg(target_arch = "x86_64")]
            row: 0,
            #[cfg(target_arch = "x86_64")]
            color: 0x0F, // White on black
            
            #[cfg(target_arch = "aarch64")]
            x: 0,
            #[cfg(target_arch = "aarch64")]
            y: 0,
        }
    }
}

#[cfg(target_arch = "x86_64")]
mod vga {
    use super::*;
    
    const VGA_BUFFER: usize = 0xB8000;
    const VGA_WIDTH: usize = 80;
    const VGA_HEIGHT: usize = 25;
    
    impl Console {
        pub fn write_byte(&mut self, byte: u8) {
            // Also output to serial for QEMU
            crate::arch::x86_64::serial::SERIAL.lock().write_byte(byte);
            
            match byte {
                b'\n' => {
                    self.col = 0;
                    self.row += 1;
                }
                b'\r' => {
                    self.col = 0;
                }
                b'\t' => {
                    self.col = (self.col + 8) & !7;
                }
                0x08 => {
                    // Backspace - move cursor back and erase character
                    if self.col > 0 {
                        self.col -= 1;
                        let offset = self.row * VGA_WIDTH + self.col;
                        let ptr = VGA_BUFFER as *mut u16;
                        unsafe {
                            ptr.add(offset).write_volatile((self.color as u16) << 8 | b' ' as u16);
                        }
                    }
                }
                byte => {
                    // Skip non-ASCII characters (> 127)
                    if byte > 127 {
                        return;
                    }
                    
                    if self.col >= VGA_WIDTH {
                        self.col = 0;
                        self.row += 1;
                    }
                    
                    if self.row >= VGA_HEIGHT {
                        self.scroll();
                    }
                    
                    let offset = self.row * VGA_WIDTH + self.col;
                    let ptr = VGA_BUFFER as *mut u16;
                    
                    unsafe {
                        ptr.add(offset).write_volatile((self.color as u16) << 8 | byte as u16);
                    }
                    
                    self.col += 1;
                }
            }
            
            if self.row >= VGA_HEIGHT {
                self.scroll();
            }
        }
        
        pub fn write_str(&mut self, s: &str) {
            for byte in s.bytes() {
                self.write_byte(byte);
            }
        }
        
        fn scroll(&mut self) {
            let ptr = VGA_BUFFER as *mut u16;
            
            // Move lines up
            for row in 1..VGA_HEIGHT {
                for col in 0..VGA_WIDTH {
                    let src = row * VGA_WIDTH + col;
                    let dst = (row - 1) * VGA_WIDTH + col;
                    unsafe {
                        let val = ptr.add(src).read_volatile();
                        ptr.add(dst).write_volatile(val);
                    }
                }
            }
            
            // Clear last line
            let blank = (self.color as u16) << 8 | b' ' as u16;
            for col in 0..VGA_WIDTH {
                let offset = (VGA_HEIGHT - 1) * VGA_WIDTH + col;
                unsafe {
                    ptr.add(offset).write_volatile(blank);
                }
            }
            
            self.row = VGA_HEIGHT - 1;
        }
        
        pub fn clear(&mut self) {
            let ptr = VGA_BUFFER as *mut u16;
            let blank = (self.color as u16) << 8 | b' ' as u16;
            
            for i in 0..(VGA_WIDTH * VGA_HEIGHT) {
                unsafe {
                    ptr.add(i).write_volatile(blank);
                }
            }
            
            self.col = 0;
            self.row = 0;
        }
        
        pub fn set_color(&mut self, fg: u8, bg: u8) {
            self.color = (bg << 4) | (fg & 0x0F);
        }
    }
}

#[cfg(target_arch = "aarch64")]
mod framebuffer {
    use super::*;
    
    // Placeholder for framebuffer console
    impl Console {
        pub fn write_byte(&mut self, byte: u8) {
            // Use UART for now
            crate::arch::aarch64::uart::write_byte(byte);
        }
        
        pub fn write_str(&mut self, s: &str) {
            for byte in s.bytes() {
                self.write_byte(byte);
            }
        }
        
        pub fn clear(&mut self) {
            // Clear screen escape sequence
            self.write_str("\x1B[2J\x1B[H");
            self.x = 0;
            self.y = 0;
        }
        
        pub fn set_color(&mut self, _fg: u8, _bg: u8) {
            // ANSI colors could be used here
        }
    }
}

impl core::fmt::Write for Console {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.write_str(s);
        Ok(())
    }
}

/// Initialize console
pub fn init() {
    {
        let mut console = CONSOLE.lock();
        console.clear();
    } // Lock released here
    crate::kprintln!("[CONSOLE] Console initialized");
}

/// Print to console
pub fn print(args: core::fmt::Arguments) {
    use core::fmt::Write;
    CONSOLE.lock().write_fmt(args).unwrap();
}
