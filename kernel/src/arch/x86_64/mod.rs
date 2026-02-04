//! x86_64 architecture support

pub mod gdt;
pub mod idt;
pub mod paging;
pub mod cpu;
pub mod apic;
pub mod pit;
pub mod serial;

use crate::BootInfo;

/// Initialize x86_64-specific components
pub fn init(boot_info: &BootInfo) {
    // GDT is already initialized by boot stub, skip re-init
    #[cfg(target_arch = "x86_64")]
    crate::early_serial_write(b"Using boot GDT\r\n");
    
    // Initialize IDT (Interrupt Descriptor Table)
    #[cfg(target_arch = "x86_64")]
    crate::early_serial_write(b"IDT init...\r\n");
    idt::init();
    #[cfg(target_arch = "x86_64")]
    crate::early_serial_write(b"IDT done\r\n");
    
    // Initialize paging
    #[cfg(target_arch = "x86_64")]
    crate::early_serial_write(b"Paging init...\r\n");
    paging::init(boot_info);
    #[cfg(target_arch = "x86_64")]
    crate::early_serial_write(b"Paging done\r\n");
    
    // Skip APIC for now - use legacy PIC for keyboard/timer interrupts
    // The APIC masks LINT0/LINT1 which breaks PIC routing
    // TODO: Implement proper I/O APIC configuration for external interrupts
    #[cfg(target_arch = "x86_64")]
    crate::early_serial_write(b"Using legacy PIC\r\n");
    
    // Initialize PIT for timer
    #[cfg(target_arch = "x86_64")]
    crate::early_serial_write(b"PIT init...\r\n");
    pit::init(1000); // 1000 Hz
    #[cfg(target_arch = "x86_64")]
    crate::early_serial_write(b"PIT done\r\n");
    
    // Initialize serial port for debugging
    #[cfg(target_arch = "x86_64")]
    crate::early_serial_write(b"Serial init...\r\n");
    serial::init();
    #[cfg(target_arch = "x86_64")]
    crate::early_serial_write(b"Serial done\r\n");
    
    // Enable interrupts
    #[cfg(target_arch = "x86_64")]
    crate::early_serial_write(b"Enabling interrupts...\r\n");
    crate::arch::enable_interrupts();
    #[cfg(target_arch = "x86_64")]
    crate::early_serial_write(b"Interrupts enabled\r\n");
}

/// Read from port
#[inline]
pub fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {
        core::arch::asm!(
            "in al, dx",
            out("al") value,
            in("dx") port,
            options(nomem, nostack)
        );
    }
    value
}

/// Write to port
#[inline]
pub fn outb(port: u16, value: u8) {
    unsafe {
        core::arch::asm!(
            "out dx, al",
            in("dx") port,
            in("al") value,
            options(nomem, nostack)
        );
    }
}

/// Read 16-bit value from port
#[inline]
pub fn inw(port: u16) -> u16 {
    let value: u16;
    unsafe {
        core::arch::asm!(
            "in ax, dx",
            out("ax") value,
            in("dx") port,
            options(nomem, nostack)
        );
    }
    value
}

/// Write 16-bit value to port
#[inline]
pub fn outw(port: u16, value: u16) {
    unsafe {
        core::arch::asm!(
            "out dx, ax",
            in("dx") port,
            in("ax") value,
            options(nomem, nostack)
        );
    }
}

/// Read 32-bit value from port
#[inline]
pub fn inl(port: u16) -> u32 {
    let value: u32;
    unsafe {
        core::arch::asm!(
            "in eax, dx",
            out("eax") value,
            in("dx") port,
            options(nomem, nostack)
        );
    }
    value
}

/// Write 32-bit value to port
#[inline]
pub fn outl(port: u16, value: u32) {
    unsafe {
        core::arch::asm!(
            "out dx, eax",
            in("dx") port,
            in("eax") value,
            options(nomem, nostack)
        );
    }
}

/// Read MSR (Model Specific Register)
#[inline]
pub fn rdmsr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    unsafe {
        core::arch::asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") low,
            out("edx") high,
            options(nomem, nostack)
        );
    }
    ((high as u64) << 32) | (low as u64)
}

/// Write MSR (Model Specific Register)
#[inline]
pub fn wrmsr(msr: u32, value: u64) {
    let low = value as u32;
    let high = (value >> 32) as u32;
    unsafe {
        core::arch::asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") low,
            in("edx") high,
            options(nomem, nostack)
        );
    }
}

/// Read CR0 register
#[inline]
pub fn read_cr0() -> u64 {
    let value: u64;
    unsafe {
        core::arch::asm!("mov {}, cr0", out(reg) value, options(nomem, nostack));
    }
    value
}

/// Write CR0 register
#[inline]
pub fn write_cr0(value: u64) {
    unsafe {
        core::arch::asm!("mov cr0, {}", in(reg) value, options(nomem, nostack));
    }
}

/// Read CR2 register (page fault address)
#[inline]
pub fn read_cr2() -> u64 {
    let value: u64;
    unsafe {
        core::arch::asm!("mov {}, cr2", out(reg) value, options(nomem, nostack));
    }
    value
}

/// Read CR3 register (page table base)
#[inline]
pub fn read_cr3() -> u64 {
    let value: u64;
    unsafe {
        core::arch::asm!("mov {}, cr3", out(reg) value, options(nomem, nostack));
    }
    value
}

/// Write CR3 register
#[inline]
pub fn write_cr3(value: u64) {
    unsafe {
        core::arch::asm!("mov cr3, {}", in(reg) value, options(nomem, nostack));
    }
}

/// Read CR4 register
#[inline]
pub fn read_cr4() -> u64 {
    let value: u64;
    unsafe {
        core::arch::asm!("mov {}, cr4", out(reg) value, options(nomem, nostack));
    }
    value
}

/// Write CR4 register
#[inline]
pub fn write_cr4(value: u64) {
    unsafe {
        core::arch::asm!("mov cr4, {}", in(reg) value, options(nomem, nostack));
    }
}

/// Invalidate TLB entry for address
#[inline]
pub fn invlpg(addr: u64) {
    unsafe {
        core::arch::asm!("invlpg [{}]", in(reg) addr, options(nostack));
    }
}

/// Get CPU features using CPUID
pub fn cpuid(leaf: u32) -> (u32, u32, u32, u32) {
    let (eax, ebx, ecx, edx): (u32, u32, u32, u32);
    unsafe {
        core::arch::asm!(
            "push rbx",
            "cpuid",
            "mov {ebx_out:e}, ebx",
            "pop rbx",
            inout("eax") leaf => eax,
            ebx_out = out(reg) ebx,
            out("ecx") ecx,
            out("edx") edx,
            options(nomem, nostack)
        );
    }
    (eax, ebx, ecx, edx)
}
