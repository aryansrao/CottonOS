//! CottonOS x86 Bootloader - Rust Components

#![no_std]
#![no_main]

mod vga;

use core::panic::PanicInfo;

/// Bootloader information structure passed to kernel
#[repr(C)]
pub struct BootInfo {
    pub magic: u32,
    pub memory_map: *const MemoryMapEntry,
    pub memory_map_entries: usize,
    pub framebuffer: FramebufferInfo,
    pub arch: Architecture,
    pub kernel_start: u64,
    pub kernel_end: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MemoryMapEntry {
    pub base: u64,
    pub length: u64,
    pub mem_type: MemoryType,
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq)]
pub enum MemoryType {
    Available = 1,
    Reserved = 2,
    AcpiReclaimable = 3,
    AcpiNvs = 4,
    BadMemory = 5,
    Kernel = 6,
    Bootloader = 7,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FramebufferInfo {
    pub address: u64,
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    pub bpp: u8,
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq)]
pub enum Architecture {
    X86,
    X86_64,
    Arm32,
    Arm64,
    Unknown,
}

/// Panic handler
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
