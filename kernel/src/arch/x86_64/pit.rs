//! Programmable Interval Timer (PIT) for x86

use crate::arch::x86_64::{outb, inb};

/// PIT ports
const PIT_CH0_DATA: u16 = 0x40;
const PIT_CH1_DATA: u16 = 0x41;
const PIT_CH2_DATA: u16 = 0x42;
const PIT_CMD: u16 = 0x43;

/// PIT base frequency (1.193182 MHz)
const PIT_FREQUENCY: u32 = 1193182;

/// Current tick count
static mut TICK_COUNT: u64 = 0;

/// Timer frequency in Hz
static mut TIMER_FREQ: u32 = 1000;

/// Initialize PIT with given frequency
pub fn init(freq: u32) {
    unsafe {
        TIMER_FREQ = freq;
    }
    
    let divisor = PIT_FREQUENCY / freq;
    
    // Set mode: Channel 0, lobyte/hibyte, square wave generator
    outb(PIT_CMD, 0x36);
    
    // Set divisor
    outb(PIT_CH0_DATA, (divisor & 0xFF) as u8);
    outb(PIT_CH0_DATA, ((divisor >> 8) & 0xFF) as u8);
}

/// Get current tick count
pub fn ticks() -> u64 {
    unsafe { TICK_COUNT }
}

/// Increment tick count (called from timer interrupt)
pub fn tick() {
    unsafe {
        TICK_COUNT += 1;
    }
}

/// Get timer frequency
pub fn frequency() -> u32 {
    unsafe { TIMER_FREQ }
}

/// Get uptime in milliseconds
pub fn uptime_ms() -> u64 {
    let ticks = ticks();
    let freq = frequency() as u64;
    (ticks * 1000) / freq
}

/// Get uptime in seconds
pub fn uptime_secs() -> u64 {
    ticks() / frequency() as u64
}

/// Sleep for given number of milliseconds (busy wait)
pub fn sleep_ms(ms: u64) {
    let target = uptime_ms() + ms;
    while uptime_ms() < target {
        crate::arch::halt();
    }
}

/// Read PIT channel 0 current count
pub fn read_count() -> u16 {
    // Latch channel 0
    outb(PIT_CMD, 0x00);
    
    let low = inb(PIT_CH0_DATA);
    let high = inb(PIT_CH0_DATA);
    
    ((high as u16) << 8) | (low as u16)
}

/// Enable PC speaker with given frequency
pub fn speaker_on(freq: u32) {
    let divisor = PIT_FREQUENCY / freq;
    
    // Set channel 2 mode
    outb(PIT_CMD, 0xB6);
    outb(PIT_CH2_DATA, (divisor & 0xFF) as u8);
    outb(PIT_CH2_DATA, ((divisor >> 8) & 0xFF) as u8);
    
    // Enable speaker
    let tmp = inb(0x61);
    outb(0x61, tmp | 0x03);
}

/// Disable PC speaker
pub fn speaker_off() {
    let tmp = inb(0x61);
    outb(0x61, tmp & 0xFC);
}
