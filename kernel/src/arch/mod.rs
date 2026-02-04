//! Architecture-specific code for x86_64
//! 
//! CottonOS is designed specifically for x86_64 architecture.

pub mod x86_64;

// Re-export x86_64 module
pub use x86_64::*;

use crate::BootInfo;

/// Initialize architecture-specific components
pub fn init(boot_info: &BootInfo) {
    x86_64::init(boot_info);
}

/// Disable interrupts
#[inline(always)]
pub fn disable_interrupts() {
    unsafe {
        core::arch::asm!("cli", options(nomem, nostack));
    }
}

/// Enable interrupts
#[inline(always)]
pub fn enable_interrupts() {
    unsafe {
        core::arch::asm!("sti", options(nomem, nostack));
    }
}

/// Halt the CPU
#[inline(always)]
pub fn halt() {
    unsafe {
        core::arch::asm!("hlt", options(nomem, nostack));
    }
}

/// Check if interrupts are enabled
#[inline(always)]
pub fn interrupts_enabled() -> bool {
    let flags: usize;
    unsafe {
        core::arch::asm!("pushfq; pop {}", out(reg) flags, options(nomem));
    }
    (flags & (1 << 9)) != 0
}

/// Execute code with interrupts disabled
pub fn without_interrupts<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    let enabled = interrupts_enabled();
    if enabled {
        disable_interrupts();
    }
    
    let result = f();
    
    if enabled {
        enable_interrupts();
    }
    
    result
}
