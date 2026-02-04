//! APIC (Advanced Programmable Interrupt Controller) support

use crate::arch::x86_64::{rdmsr, wrmsr, cpuid};

/// APIC base MSR
const IA32_APIC_BASE_MSR: u32 = 0x1B;

/// APIC register offsets
mod regs {
    pub const ID: u32 = 0x020;
    pub const VERSION: u32 = 0x030;
    pub const TPR: u32 = 0x080;       // Task Priority Register
    pub const EOI: u32 = 0x0B0;       // End of Interrupt
    pub const SVR: u32 = 0x0F0;       // Spurious Vector Register
    pub const ICR_LOW: u32 = 0x300;   // Interrupt Command Register (low)
    pub const ICR_HIGH: u32 = 0x310;  // Interrupt Command Register (high)
    pub const LVT_TIMER: u32 = 0x320;
    pub const LVT_THERMAL: u32 = 0x330;
    pub const LVT_PERF: u32 = 0x340;
    pub const LVT_LINT0: u32 = 0x350;
    pub const LVT_LINT1: u32 = 0x360;
    pub const LVT_ERROR: u32 = 0x370;
    pub const TIMER_ICR: u32 = 0x380; // Timer Initial Count Register
    pub const TIMER_CCR: u32 = 0x390; // Timer Current Count Register
    pub const TIMER_DCR: u32 = 0x3E0; // Timer Divide Configuration Register
}

/// Local APIC base address (default)
static mut APIC_BASE: u64 = 0xFEE00000;

/// Check if APIC is available
pub fn is_available() -> bool {
    let (_, _, _, edx) = cpuid(1);
    (edx & (1 << 9)) != 0
}

/// Initialize Local APIC
pub fn init() -> bool {
    if !is_available() {
        return false;
    }
    
    unsafe {
        // Get APIC base address
        let base = rdmsr(IA32_APIC_BASE_MSR);
        APIC_BASE = base & 0xFFFF_F000;
        
        // Enable APIC
        wrmsr(IA32_APIC_BASE_MSR, base | (1 << 11));
        
        // Enable spurious interrupts (vector 0xFF)
        write_reg(regs::SVR, 0x1FF);
        
        // Set task priority to 0 (accept all interrupts)
        write_reg(regs::TPR, 0);
        
        // Mask all LVT entries initially
        write_reg(regs::LVT_TIMER, 1 << 16);
        write_reg(regs::LVT_THERMAL, 1 << 16);
        write_reg(regs::LVT_PERF, 1 << 16);
        write_reg(regs::LVT_LINT0, 1 << 16);
        write_reg(regs::LVT_LINT1, 1 << 16);
        write_reg(regs::LVT_ERROR, 1 << 16);
    }
    
    true
}

/// Read APIC register
fn read_reg(offset: u32) -> u32 {
    unsafe {
        let addr = (APIC_BASE + offset as u64) as *const u32;
        core::ptr::read_volatile(addr)
    }
}

/// Write APIC register
fn write_reg(offset: u32, value: u32) {
    unsafe {
        let addr = (APIC_BASE + offset as u64) as *mut u32;
        core::ptr::write_volatile(addr, value);
    }
}

/// Send End of Interrupt
pub fn send_eoi() {
    write_reg(regs::EOI, 0);
}

/// Get APIC ID
pub fn get_id() -> u8 {
    ((read_reg(regs::ID) >> 24) & 0xFF) as u8
}

/// Get APIC version
pub fn get_version() -> u8 {
    (read_reg(regs::VERSION) & 0xFF) as u8
}

/// Configure APIC timer
pub fn configure_timer(vector: u8, divider: u8, initial_count: u32, periodic: bool) {
    // Set divider
    let dcr = match divider {
        1 => 0xB,
        2 => 0x0,
        4 => 0x1,
        8 => 0x2,
        16 => 0x3,
        32 => 0x8,
        64 => 0x9,
        128 => 0xA,
        _ => 0xB, // Default to divide by 1
    };
    write_reg(regs::TIMER_DCR, dcr);
    
    // Configure LVT timer entry
    let mode = if periodic { 0x20000 } else { 0 };
    write_reg(regs::LVT_TIMER, (vector as u32) | mode);
    
    // Set initial count to start timer
    write_reg(regs::TIMER_ICR, initial_count);
}

/// Stop APIC timer
pub fn stop_timer() {
    write_reg(regs::TIMER_ICR, 0);
    write_reg(regs::LVT_TIMER, 1 << 16);
}

/// Get timer current count
pub fn timer_current() -> u32 {
    read_reg(regs::TIMER_CCR)
}

/// Send Inter-Processor Interrupt (IPI)
pub fn send_ipi(apic_id: u8, vector: u8) {
    write_reg(regs::ICR_HIGH, (apic_id as u32) << 24);
    write_reg(regs::ICR_LOW, vector as u32);
}

/// Send Init IPI to all processors
pub fn send_init_ipi_all() {
    write_reg(regs::ICR_HIGH, 0);
    write_reg(regs::ICR_LOW, 0xC4500);
}

/// Send Startup IPI to all processors
pub fn send_startup_ipi_all(vector: u8) {
    write_reg(regs::ICR_HIGH, 0);
    write_reg(regs::ICR_LOW, 0xC4600 | (vector as u32));
}
