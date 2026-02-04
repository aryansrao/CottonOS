//! Kernel Heap Allocator
//!
//! Provides dynamic memory allocation for the kernel using linked_list_allocator.

use linked_list_allocator::LockedHeap;
use crate::mm::{PAGE_SIZE, physical};

/// Heap start address (identity mapped in low memory for early boot)
const HEAP_START: u64 = 0x0000_0000_0200_0000; // 32MB - well above kernel at 1MB

/// Initial heap size (4MB) - needs to be large enough for GUI back buffer
const HEAP_SIZE: usize = 4 * 1024 * 1024;

/// Maximum heap size (16MB)
const MAX_HEAP_SIZE: usize = 16 * 1024 * 1024;

/// Global allocator
#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

/// Current heap end
static mut HEAP_END: u64 = HEAP_START;

/// Initialize heap allocator
pub fn init() {
    // Allocate physical pages for initial heap
    let num_pages = (HEAP_SIZE + PAGE_SIZE - 1) / PAGE_SIZE;
    
    for i in 0..num_pages {
        let phys = physical::alloc_frame().expect("Failed to allocate heap page");
        let virt = HEAP_START + (i * PAGE_SIZE) as u64;
        
        #[cfg(target_arch = "x86_64")]
        {
            use crate::arch::x86_64::paging::flags;
            let _ = crate::arch::x86_64::paging::map_page(
                virt,
                phys,
                flags::PRESENT | flags::WRITABLE | flags::NO_EXECUTE
            );
        }
        
        #[cfg(target_arch = "aarch64")]
        {
            use crate::arch::aarch64::mmu::flags;
            let _ = crate::arch::aarch64::mmu::map_page(
                virt,
                phys,
                flags::AP_RW_EL1 | flags::ATTR_NORMAL
            );
        }
    }
    
    unsafe {
        HEAP_END = HEAP_START + HEAP_SIZE as u64;
        ALLOCATOR.lock().init(HEAP_START as *mut u8, HEAP_SIZE);
    }
}

/// Extend heap by given size
pub fn extend_heap(additional: usize) -> Result<(), &'static str> {
    unsafe {
        if HEAP_END - HEAP_START + additional as u64 > MAX_HEAP_SIZE as u64 {
            return Err("Maximum heap size exceeded");
        }
        
        let num_pages = (additional + PAGE_SIZE - 1) / PAGE_SIZE;
        
        for i in 0..num_pages {
            let phys = physical::alloc_frame().ok_or("Out of physical memory")?;
            let virt = HEAP_END + (i * PAGE_SIZE) as u64;
            
            #[cfg(target_arch = "x86_64")]
            {
                use crate::arch::x86_64::paging::flags;
                crate::arch::x86_64::paging::map_page(
                    virt,
                    phys,
                    flags::PRESENT | flags::WRITABLE | flags::NO_EXECUTE
                )?;
            }
            
            #[cfg(target_arch = "aarch64")]
            {
                use crate::arch::aarch64::mmu::flags;
                crate::arch::aarch64::mmu::map_page(
                    virt,
                    phys,
                    flags::AP_RW_EL1 | flags::ATTR_NORMAL
                )?;
            }
        }
        
        ALLOCATOR.lock().extend(num_pages * PAGE_SIZE);
        HEAP_END += (num_pages * PAGE_SIZE) as u64;
        
        Ok(())
    }
}

/// Get heap statistics
pub fn heap_stats() -> (usize, usize) {
    let allocator = ALLOCATOR.lock();
    (allocator.free(), allocator.used())
}

/// Get heap size
pub fn heap_size() -> usize {
    unsafe { (HEAP_END - HEAP_START) as usize }
}
