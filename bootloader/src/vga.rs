//! VGA text mode driver for bootloader

#![allow(dead_code)]

const VGA_BUFFER: *mut u16 = 0xB8000 as *mut u16;
const VGA_WIDTH: usize = 80;
const VGA_HEIGHT: usize = 25;

#[repr(u8)]
#[derive(Clone, Copy)]
pub enum Color {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

pub struct VgaWriter {
    col: usize,
    row: usize,
    color: u8,
}

impl VgaWriter {
    pub const fn new() -> Self {
        Self {
            col: 0,
            row: 0,
            color: (Color::White as u8) | ((Color::Black as u8) << 4),
        }
    }

    pub fn clear(&mut self) {
        let blank = (self.color as u16) << 8 | b' ' as u16;
        for i in 0..(VGA_WIDTH * VGA_HEIGHT) {
            unsafe {
                VGA_BUFFER.add(i).write_volatile(blank);
            }
        }
        self.col = 0;
        self.row = 0;
    }

    pub fn write_char(&mut self, c: u8) {
        match c {
            b'\n' => {
                self.col = 0;
                self.row += 1;
            }
            b'\r' => {
                self.col = 0;
            }
            _ => {
                let idx = self.row * VGA_WIDTH + self.col;
                if idx < VGA_WIDTH * VGA_HEIGHT {
                    let value = (self.color as u16) << 8 | c as u16;
                    unsafe {
                        VGA_BUFFER.add(idx).write_volatile(value);
                    }
                }
                self.col += 1;
            }
        }

        if self.col >= VGA_WIDTH {
            self.col = 0;
            self.row += 1;
        }

        if self.row >= VGA_HEIGHT {
            self.scroll();
        }
    }

    pub fn write_string(&mut self, s: &str) {
        for byte in s.bytes() {
            self.write_char(byte);
        }
    }

    fn scroll(&mut self) {
        for row in 1..VGA_HEIGHT {
            for col in 0..VGA_WIDTH {
                let src = row * VGA_WIDTH + col;
                let dst = (row - 1) * VGA_WIDTH + col;
                unsafe {
                    let value = VGA_BUFFER.add(src).read_volatile();
                    VGA_BUFFER.add(dst).write_volatile(value);
                }
            }
        }

        let blank = (self.color as u16) << 8 | b' ' as u16;
        for col in 0..VGA_WIDTH {
            let idx = (VGA_HEIGHT - 1) * VGA_WIDTH + col;
            unsafe {
                VGA_BUFFER.add(idx).write_volatile(blank);
            }
        }

        self.row = VGA_HEIGHT - 1;
    }

    pub fn set_color(&mut self, fg: Color, bg: Color) {
        self.color = (fg as u8) | ((bg as u8) << 4);
    }
}
