//! Serial port driver for x86_64

use crate::arch::x86_64::{inb, outb};
use core::fmt::{self, Write};
use spin::Mutex;

/// COM1 port address
const COM1: u16 = 0x3F8;

/// Serial port structure
pub struct Serial {
    port: u16,
}

impl Serial {
    /// Create new serial port
    pub const fn new(port: u16) -> Self {
        Self { port }
    }

    /// Initialize serial port
    pub fn init(&self) {
        // Disable interrupts
        outb(self.port + 1, 0x00);
        
        // Enable DLAB (Divisor Latch Access Bit)
        outb(self.port + 3, 0x80);
        
        // Set baud rate divisor (115200 baud)
        outb(self.port + 0, 0x01); // Low byte
        outb(self.port + 1, 0x00); // High byte
        
        // 8 bits, no parity, one stop bit
        outb(self.port + 3, 0x03);
        
        // Enable FIFO, clear, 14-byte threshold
        outb(self.port + 2, 0xC7);
        
        // IRQs enabled, RTS/DSR set
        outb(self.port + 4, 0x0B);
        
        // Set in loopback mode to test
        outb(self.port + 4, 0x1E);
        
        // Test serial chip
        outb(self.port + 0, 0xAE);
        
        // Check if serial is faulty
        if inb(self.port + 0) != 0xAE {
            return; // Serial is faulty
        }
        
        // Set normal operation mode
        outb(self.port + 4, 0x0F);
    }

    /// Check if transmit buffer is empty
    fn is_transmit_empty(&self) -> bool {
        inb(self.port + 5) & 0x20 != 0
    }

    /// Write a byte to serial port
    pub fn write_byte(&self, byte: u8) {
        while !self.is_transmit_empty() {}
        outb(self.port, byte);
    }

    /// Check if data is available
    fn has_data(&self) -> bool {
        inb(self.port + 5) & 0x01 != 0
    }

    /// Read a byte from serial port
    pub fn read_byte(&self) -> Option<u8> {
        if self.has_data() {
            Some(inb(self.port))
        } else {
            None
        }
    }

    /// Write string to serial port
    pub fn write_string(&self, s: &str) {
        for byte in s.bytes() {
            if byte == b'\n' {
                self.write_byte(b'\r');
            }
            self.write_byte(byte);
        }
    }
}

impl Write for Serial {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

/// Global serial port
pub static SERIAL: Mutex<Serial> = Mutex::new(Serial::new(COM1));

/// Initialize serial port
pub fn init() {
    SERIAL.lock().init();
}

/// Serial print macros
#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => ({
        use core::fmt::Write;
        let _ = write!($crate::arch::x86_64::serial::SERIAL.lock(), $($arg)*);
    });
}

#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($($arg:tt)*) => ($crate::serial_print!("{}\n", format_args!($($arg)*)));
}
