//! Memory Management Module
//! 
//! This module handles all memory-related operations including:
//! - Physical memory allocation
//! - Virtual memory management
//! - Heap allocation

pub mod physical;
pub mod virtual_mem;
pub mod heap;

use crate::BootInfo;
use spin::Mutex;

/// Page size (4KB)
pub const PAGE_SIZE: usize = 4096;

/// Page shift
pub const PAGE_SHIFT: usize = 12;

/// Memory region types
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum MemoryType {
    Available = 1,
    Reserved = 2,
    AcpiReclaimable = 3,
    AcpiNvs = 4,
    BadMemory = 5,
    Kernel = 6,
    Bootloader = 7,
    Framebuffer = 8,
    PageTables = 9,
}

/// Memory map entry from bootloader
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MemoryMapEntry {
    pub base: u64,
    pub length: u64,
    pub mem_type: MemoryType,
}

/// Memory statistics
pub struct MemoryStats {
    pub total_memory: u64,
    pub available_memory: u64,
    pub used_memory: u64,
    pub free_pages: u64,
    pub used_pages: u64,
}

/// Global memory statistics
static MEMORY_STATS: Mutex<MemoryStats> = Mutex::new(MemoryStats {
    total_memory: 0,
    available_memory: 0,
    used_memory: 0,
    free_pages: 0,
    used_pages: 0,
});

/// Initialize memory management
pub fn init(boot_info: &BootInfo) {
    // Parse memory map
    parse_memory_map(boot_info);
    
    // Initialize physical memory allocator
    physical::init(boot_info);
    crate::kprintln!("[MM] Physical memory allocator initialized");
    
    // Initialize virtual memory
    virtual_mem::init();
    crate::kprintln!("[MM] Virtual memory initialized");
    
    // Initialize heap
    heap::init();
    crate::kprintln!("[MM] Heap initialized");
    
    // Print memory statistics
    let stats = MEMORY_STATS.lock();
    crate::kprintln!("[MM] Total memory: {} MB", stats.total_memory / (1024 * 1024));
    crate::kprintln!("[MM] Available memory: {} MB", stats.available_memory / (1024 * 1024));
}

/// Parse memory map from bootloader
fn parse_memory_map(boot_info: &BootInfo) {
    let mut stats = MEMORY_STATS.lock();
    
    if boot_info.memory_map.is_null() || boot_info.memory_map_entries == 0 {
        // No memory map provided, assume 128MB for QEMU
        stats.total_memory = 128 * 1024 * 1024;
        stats.available_memory = 64 * 1024 * 1024;
        return;
    }
    
    unsafe {
        for i in 0..boot_info.memory_map_entries {
            let entry = &*boot_info.memory_map.add(i);
            
            stats.total_memory += entry.length;
            
            if entry.mem_type == MemoryType::Available {
                stats.available_memory += entry.length;
            }
        }
    }
}

/// Align address down to page boundary
#[inline]
pub const fn page_align_down(addr: u64) -> u64 {
    addr & !(PAGE_SIZE as u64 - 1)
}

/// Align address up to page boundary
#[inline]
pub const fn page_align_up(addr: u64) -> u64 {
    (addr + PAGE_SIZE as u64 - 1) & !(PAGE_SIZE as u64 - 1)
}

/// Convert address to page number
#[inline]
pub const fn addr_to_page(addr: u64) -> u64 {
    addr >> PAGE_SHIFT
}

/// Convert page number to address
#[inline]
pub const fn page_to_addr(page: u64) -> u64 {
    page << PAGE_SHIFT
}

/// Get memory statistics
pub fn get_stats() -> MemoryStats {
    let stats = MEMORY_STATS.lock();
    MemoryStats {
        total_memory: stats.total_memory,
        available_memory: stats.available_memory,
        used_memory: stats.used_memory,
        free_pages: stats.free_pages,
        used_pages: stats.used_pages,
    }
}

/// Update memory statistics
pub fn update_stats(free_pages: u64, used_pages: u64) {
    let mut stats = MEMORY_STATS.lock();
    stats.free_pages = free_pages;
    stats.used_pages = used_pages;
    stats.used_memory = used_pages * PAGE_SIZE as u64;
}
