//! Virtual Memory Management
//!
//! Manages virtual address spaces, page mapping, and memory regions.

use crate::mm::{PAGE_SIZE, physical};
use spin::Mutex;
use alloc::vec::Vec;

/// Virtual memory region
#[derive(Clone)]
pub struct VmRegion {
    pub start: u64,
    pub end: u64,
    pub flags: VmFlags,
    pub name: &'static str,
}

bitflags::bitflags! {
    /// Virtual memory flags
    #[derive(Clone, Copy, Debug)]
    pub struct VmFlags: u32 {
        const READ = 1 << 0;
        const WRITE = 1 << 1;
        const EXECUTE = 1 << 2;
        const USER = 1 << 3;
        const SHARED = 1 << 4;
        const STACK = 1 << 5;
        const HEAP = 1 << 6;
        const MMIO = 1 << 7;
    }
}

/// Virtual address space
pub struct AddressSpace {
    /// Page table root (physical address)
    pub page_table_root: u64,
    /// Memory regions
    regions: Vec<VmRegion>,
    /// Process ID owning this address space
    pub pid: u32,
}

impl AddressSpace {
    /// Create a new address space
    pub fn new(pid: u32) -> Option<Self> {
        let page_table_root = physical::alloc_frame()?;
        
        // Zero out the page table
        unsafe {
            let ptr = page_table_root as *mut u8;
            core::ptr::write_bytes(ptr, 0, PAGE_SIZE);
        }
        
        Some(Self {
            page_table_root,
            regions: Vec::new(),
            pid,
        })
    }
    
    /// Map a region of memory
    pub fn map_region(&mut self, start: u64, size: u64, flags: VmFlags, name: &'static str) -> Result<(), &'static str> {
        let end = start + size;
        
        // Check for overlaps
        for region in &self.regions {
            if start < region.end && end > region.start {
                return Err("Region overlaps with existing region");
            }
        }
        
        // Add region
        self.regions.push(VmRegion {
            start,
            end,
            flags,
            name,
        });
        
        // Map pages
        let num_pages = (size as usize + PAGE_SIZE - 1) / PAGE_SIZE;
        for i in 0..num_pages {
            let virt = start + (i * PAGE_SIZE) as u64;
            let phys = physical::alloc_frame().ok_or("Out of physical memory")?;
            
            self.map_page(virt, phys, flags)?;
        }
        
        Ok(())
    }
    
    /// Map a single page
    pub fn map_page(&mut self, virt: u64, phys: u64, flags: VmFlags) -> Result<(), &'static str> {
        #[cfg(target_arch = "x86_64")]
        {
            let arch_flags = self.vm_to_arch_flags_x86(flags);
            crate::arch::x86_64::paging::map_page(virt, phys, arch_flags)
        }
        
        #[cfg(target_arch = "aarch64")]
        {
            let arch_flags = self.vm_to_arch_flags_arm(flags);
            crate::arch::aarch64::mmu::map_page(virt, phys, arch_flags)
        }
        
        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        {
            Ok(())
        }
    }
    
    #[cfg(target_arch = "x86_64")]
    fn vm_to_arch_flags_x86(&self, flags: VmFlags) -> u64 {
        use crate::arch::x86_64::paging::flags as pf;
        
        let mut arch_flags = pf::PRESENT;
        
        if flags.contains(VmFlags::WRITE) {
            arch_flags |= pf::WRITABLE;
        }
        if flags.contains(VmFlags::USER) {
            arch_flags |= pf::USER;
        }
        if !flags.contains(VmFlags::EXECUTE) {
            arch_flags |= pf::NO_EXECUTE;
        }
        if flags.contains(VmFlags::MMIO) {
            arch_flags |= pf::NO_CACHE;
        }
        
        arch_flags
    }
    
    #[cfg(target_arch = "aarch64")]
    fn vm_to_arch_flags_arm(&self, flags: VmFlags) -> u64 {
        use crate::arch::aarch64::mmu::flags as af;
        
        let mut arch_flags = af::VALID | af::AF;
        
        if flags.contains(VmFlags::USER) {
            if flags.contains(VmFlags::WRITE) {
                arch_flags |= af::AP_RW_ALL;
            } else {
                arch_flags |= af::AP_RO_ALL;
            }
        } else {
            if flags.contains(VmFlags::WRITE) {
                arch_flags |= af::AP_RW_EL1;
            } else {
                arch_flags |= af::AP_RO_EL1;
            }
        }
        
        if !flags.contains(VmFlags::EXECUTE) {
            arch_flags |= af::PXN | af::UXN;
        }
        
        if flags.contains(VmFlags::MMIO) {
            arch_flags |= af::ATTR_DEVICE;
        } else {
            arch_flags |= af::ATTR_NORMAL | af::SH_INNER;
        }
        
        arch_flags
    }
    
    /// Unmap a region
    pub fn unmap_region(&mut self, start: u64) -> Result<(), &'static str> {
        let region_idx = self.regions.iter().position(|r| r.start == start)
            .ok_or("Region not found")?;
        
        let region = self.regions.remove(region_idx);
        
        let num_pages = ((region.end - region.start) as usize + PAGE_SIZE - 1) / PAGE_SIZE;
        for i in 0..num_pages {
            let virt = region.start + (i * PAGE_SIZE) as u64;
            self.unmap_page(virt)?;
        }
        
        Ok(())
    }
    
    /// Unmap a single page
    pub fn unmap_page(&mut self, virt: u64) -> Result<(), &'static str> {
        #[cfg(target_arch = "x86_64")]
        {
            let phys = crate::arch::x86_64::paging::unmap_page(virt)?;
            physical::free_frame(phys);
        }
        
        #[cfg(target_arch = "aarch64")]
        {
            // ARM doesn't have unmap in our simple implementation
        }
        
        Ok(())
    }
    
    /// Find a free region in the address space
    pub fn find_free_region(&self, size: u64, flags: VmFlags) -> Option<u64> {
        let start_addr = if flags.contains(VmFlags::USER) {
            0x1000_0000u64 // User space starts at 256MB
        } else {
            0x0010_0000u64 // Kernel space at 1MB (identity mapped during boot)
        };
        
        let end_addr = if flags.contains(VmFlags::USER) {
            0x0000_7FFF_FFFF_0000u64 // End of user space
        } else {
            0x0000_0000_4000_0000u64 // End of kernel space (1GB identity mapped)
        };
        
        let mut current = start_addr;
        
        // Sort regions by start address for this search
        let mut sorted_regions: Vec<&VmRegion> = self.regions.iter().collect();
        sorted_regions.sort_by_key(|r| r.start);
        
        for region in sorted_regions {
            if region.start >= current && region.start - current >= size {
                return Some(current);
            }
            current = region.end;
        }
        
        // Check space after last region
        if current + size <= end_addr {
            return Some(current);
        }
        
        None
    }
}

/// Kernel address space
static KERNEL_ADDRESS_SPACE: Mutex<Option<AddressSpace>> = Mutex::new(None);

/// Initialize virtual memory
pub fn init() {
    let mut kas = KERNEL_ADDRESS_SPACE.lock();
    *kas = AddressSpace::new(0); // PID 0 for kernel
}

/// Get kernel address space
pub fn kernel_space() -> &'static Mutex<Option<AddressSpace>> {
    &KERNEL_ADDRESS_SPACE
}

/// Map pages in kernel space
pub fn kernel_map(virt: u64, phys: u64, size: u64, flags: VmFlags) -> Result<(), &'static str> {
    let mut kas = KERNEL_ADDRESS_SPACE.lock();
    if let Some(ref mut space) = *kas {
        let num_pages = (size as usize + PAGE_SIZE - 1) / PAGE_SIZE;
        for i in 0..num_pages {
            let v = virt + (i * PAGE_SIZE) as u64;
            let p = phys + (i * PAGE_SIZE) as u64;
            space.map_page(v, p, flags)?;
        }
        Ok(())
    } else {
        Err("Kernel address space not initialized")
    }
}

/// Allocate kernel memory
pub fn kernel_alloc(size: u64, flags: VmFlags) -> Option<u64> {
    let mut kas = KERNEL_ADDRESS_SPACE.lock();
    if let Some(ref mut space) = *kas {
        let virt = space.find_free_region(size, flags)?;
        space.map_region(virt, size, flags, "kernel_alloc").ok()?;
        Some(virt)
    } else {
        None
    }
}
