//! Physical Memory Allocator
//! 
//! Bitmap-based physical frame allocator supporting allocation
//! and deallocation of 4KB physical memory frames.

use crate::BootInfo;
use crate::mm::{PAGE_SIZE, MemoryType, page_align_up, page_align_down};
use spin::Mutex;

/// Maximum supported physical memory (4GB)
const MAX_PHYSICAL_MEMORY: u64 = 4 * 1024 * 1024 * 1024;

/// Maximum number of pages
const MAX_PAGES: usize = (MAX_PHYSICAL_MEMORY / PAGE_SIZE as u64) as usize;

/// Bitmap size in bytes
const BITMAP_SIZE: usize = MAX_PAGES / 8;

/// Physical frame allocator
pub struct FrameAllocator {
    /// Bitmap tracking allocated pages (1 = allocated, 0 = free)
    bitmap: [u8; BITMAP_SIZE],
    /// First free page hint for faster allocation
    first_free: usize,
    /// Total number of pages
    total_pages: usize,
    /// Number of free pages
    free_pages: usize,
}

impl FrameAllocator {
    pub const fn new() -> Self {
        Self {
            bitmap: [0xFF; BITMAP_SIZE], // Start with all pages marked as allocated
            first_free: 0,
            total_pages: 0,
            free_pages: 0,
        }
    }
    
    /// Initialize the allocator with available memory regions
    pub fn init(&mut self, boot_info: &BootInfo) {
        // Mark all pages as allocated initially
        for byte in self.bitmap.iter_mut() {
            *byte = 0xFF;
        }
        
        // If no memory map, use default range
        if boot_info.memory_map.is_null() || boot_info.memory_map_entries == 0 {
            // Default: assume 128MB starting at 1MB
            let start_page = 0x100000 / PAGE_SIZE;
            let end_page = 0x8000000 / PAGE_SIZE; // 128MB
            
            for page in start_page..end_page {
                self.mark_free(page);
            }
        } else {
            // Parse memory map
            unsafe {
                for i in 0..boot_info.memory_map_entries {
                    let entry = &*boot_info.memory_map.add(i);
                    
                    if entry.mem_type == MemoryType::Available {
                        let start = page_align_up(entry.base) as usize / PAGE_SIZE;
                        let end = page_align_down(entry.base + entry.length) as usize / PAGE_SIZE;
                        
                        for page in start..end {
                            if page < MAX_PAGES {
                                self.mark_free(page);
                            }
                        }
                    }
                }
            }
        }
        
        // Reserve first 1MB (low memory, BIOS, etc.)
        for page in 0..(0x100000 / PAGE_SIZE) {
            self.mark_allocated(page);
        }
        
        // Reserve kernel space (1MB - 2MB typical)
        let kernel_start = boot_info.kernel_start as usize / PAGE_SIZE;
        let kernel_end = (boot_info.kernel_end as usize + PAGE_SIZE - 1) / PAGE_SIZE;
        
        if kernel_start > 0 {
            for page in kernel_start..kernel_end {
                self.mark_allocated(page);
            }
        }
        
        // Find first free page
        self.first_free = 0;
        for i in 0..MAX_PAGES {
            if !self.is_allocated(i) {
                self.first_free = i;
                break;
            }
        }
        
        // Update stats
        crate::mm::update_stats(self.free_pages as u64, (self.total_pages - self.free_pages) as u64);
    }
    
    /// Mark a page as allocated
    fn mark_allocated(&mut self, page: usize) {
        if page >= MAX_PAGES {
            return;
        }
        
        let byte = page / 8;
        let bit = page % 8;
        
        if self.bitmap[byte] & (1 << bit) == 0 {
            self.bitmap[byte] |= 1 << bit;
            if self.free_pages > 0 {
                self.free_pages -= 1;
            }
        }
    }
    
    /// Mark a page as free
    fn mark_free(&mut self, page: usize) {
        if page >= MAX_PAGES {
            return;
        }
        
        let byte = page / 8;
        let bit = page % 8;
        
        if self.bitmap[byte] & (1 << bit) != 0 {
            self.bitmap[byte] &= !(1 << bit);
            self.free_pages += 1;
            self.total_pages = self.total_pages.max(page + 1);
        }
    }
    
    /// Check if a page is allocated
    fn is_allocated(&self, page: usize) -> bool {
        if page >= MAX_PAGES {
            return true;
        }
        
        let byte = page / 8;
        let bit = page % 8;
        self.bitmap[byte] & (1 << bit) != 0
    }
    
    /// Allocate a single physical frame
    pub fn alloc(&mut self) -> Option<u64> {
        // Start search from first_free hint
        for page in self.first_free..MAX_PAGES {
            if !self.is_allocated(page) {
                self.mark_allocated(page);
                self.first_free = page + 1;
                return Some((page * PAGE_SIZE) as u64);
            }
        }
        
        // Wrap around if not found
        for page in 0..self.first_free {
            if !self.is_allocated(page) {
                self.mark_allocated(page);
                self.first_free = page + 1;
                return Some((page * PAGE_SIZE) as u64);
            }
        }
        
        None
    }
    
    /// Allocate contiguous physical frames
    pub fn alloc_contiguous(&mut self, count: usize) -> Option<u64> {
        if count == 0 {
            return None;
        }
        
        if count == 1 {
            return self.alloc();
        }
        
        // Find contiguous free pages
        let mut start = self.first_free;
        let mut found = 0;
        
        for page in start..MAX_PAGES {
            if !self.is_allocated(page) {
                if found == 0 {
                    start = page;
                }
                found += 1;
                
                if found == count {
                    // Mark all pages as allocated
                    for p in start..(start + count) {
                        self.mark_allocated(p);
                    }
                    return Some((start * PAGE_SIZE) as u64);
                }
            } else {
                found = 0;
            }
        }
        
        None
    }
    
    /// Free a physical frame
    pub fn free(&mut self, addr: u64) {
        let page = addr as usize / PAGE_SIZE;
        
        if page < MAX_PAGES && self.is_allocated(page) {
            self.mark_free(page);
            
            if page < self.first_free {
                self.first_free = page;
            }
        }
    }
    
    /// Free contiguous physical frames
    pub fn free_contiguous(&mut self, addr: u64, count: usize) {
        let start_page = addr as usize / PAGE_SIZE;
        
        for i in 0..count {
            let page = start_page + i;
            if page < MAX_PAGES {
                self.mark_free(page);
            }
        }
        
        if start_page < self.first_free {
            self.first_free = start_page;
        }
    }
    
    /// Get free page count
    pub fn free_count(&self) -> usize {
        self.free_pages
    }
    
    /// Get total page count
    pub fn total_count(&self) -> usize {
        self.total_pages
    }
}

/// Global frame allocator
static FRAME_ALLOCATOR: Mutex<FrameAllocator> = Mutex::new(FrameAllocator::new());

/// Initialize physical memory allocator
pub fn init(boot_info: &BootInfo) {
    FRAME_ALLOCATOR.lock().init(boot_info);
}

/// Allocate a physical frame
pub fn alloc_frame() -> Option<u64> {
    FRAME_ALLOCATOR.lock().alloc()
}

/// Allocate contiguous physical frames
pub fn alloc_frames(count: usize) -> Option<u64> {
    FRAME_ALLOCATOR.lock().alloc_contiguous(count)
}

/// Free a physical frame
pub fn free_frame(addr: u64) {
    FRAME_ALLOCATOR.lock().free(addr);
}

/// Free contiguous physical frames
pub fn free_frames(addr: u64, count: usize) {
    FRAME_ALLOCATOR.lock().free_contiguous(addr, count);
}

/// Get free frame count
pub fn free_frames_count() -> usize {
    FRAME_ALLOCATOR.lock().free_count()
}

/// Get total frame count
pub fn total_frames_count() -> usize {
    FRAME_ALLOCATOR.lock().total_count()
}

/// Get memory statistics (total, used, free) in bytes
pub fn stats() -> (usize, usize, usize) {
    let allocator = FRAME_ALLOCATOR.lock();
    let total = allocator.total_count() * PAGE_SIZE;
    let free = allocator.free_count() * PAGE_SIZE;
    let used = total - free;
    (total, used, free)
}
