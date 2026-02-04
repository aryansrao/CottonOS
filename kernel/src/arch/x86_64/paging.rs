//! x86_64 paging support

use crate::BootInfo;

/// Page table entry flags
pub mod flags {
    pub const PRESENT: u64 = 1 << 0;
    pub const WRITABLE: u64 = 1 << 1;
    pub const USER: u64 = 1 << 2;
    pub const WRITE_THROUGH: u64 = 1 << 3;
    pub const NO_CACHE: u64 = 1 << 4;
    pub const ACCESSED: u64 = 1 << 5;
    pub const DIRTY: u64 = 1 << 6;
    pub const HUGE_PAGE: u64 = 1 << 7;
    pub const GLOBAL: u64 = 1 << 8;
    pub const NO_EXECUTE: u64 = 1 << 63;
}

/// Page table entry
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct PageTableEntry(u64);

impl PageTableEntry {
    pub const fn empty() -> Self {
        Self(0)
    }
    
    pub fn new(addr: u64, flags: u64) -> Self {
        Self((addr & 0x000F_FFFF_FFFF_F000) | flags)
    }
    
    pub fn is_present(&self) -> bool {
        self.0 & flags::PRESENT != 0
    }
    
    pub fn is_writable(&self) -> bool {
        self.0 & flags::WRITABLE != 0
    }
    
    pub fn is_user(&self) -> bool {
        self.0 & flags::USER != 0
    }
    
    pub fn is_huge(&self) -> bool {
        self.0 & flags::HUGE_PAGE != 0
    }
    
    pub fn addr(&self) -> u64 {
        self.0 & 0x000F_FFFF_FFFF_F000
    }
    
    pub fn flags(&self) -> u64 {
        self.0 & 0xFFF0_0000_0000_0FFF
    }
    
    pub fn set_addr(&mut self, addr: u64) {
        self.0 = (self.0 & 0xFFF0_0000_0000_0FFF) | (addr & 0x000F_FFFF_FFFF_F000);
    }
    
    pub fn set_flags(&mut self, new_flags: u64) {
        self.0 = (self.0 & 0x000F_FFFF_FFFF_F000) | new_flags;
    }
}

/// Page table (512 entries)
#[repr(C, align(4096))]
pub struct PageTable {
    entries: [PageTableEntry; 512],
}

impl PageTable {
    pub const fn empty() -> Self {
        Self {
            entries: [PageTableEntry::empty(); 512],
        }
    }
    
    pub fn get(&self, index: usize) -> &PageTableEntry {
        &self.entries[index]
    }
    
    pub fn get_mut(&mut self, index: usize) -> &mut PageTableEntry {
        &mut self.entries[index]
    }
}

/// Page table indices for a virtual address
pub struct PageTableIndices {
    pub pml4: usize,
    pub pdpt: usize,
    pub pd: usize,
    pub pt: usize,
    pub offset: usize,
}

impl PageTableIndices {
    pub fn from_addr(addr: u64) -> Self {
        Self {
            pml4: ((addr >> 39) & 0x1FF) as usize,
            pdpt: ((addr >> 30) & 0x1FF) as usize,
            pd: ((addr >> 21) & 0x1FF) as usize,
            pt: ((addr >> 12) & 0x1FF) as usize,
            offset: (addr & 0xFFF) as usize,
        }
    }
}

/// Kernel page tables
static mut KERNEL_PML4: PageTable = PageTable::empty();
static mut KERNEL_PDPT: PageTable = PageTable::empty();
static mut KERNEL_PD: [PageTable; 4] = [
    PageTable::empty(),
    PageTable::empty(),
    PageTable::empty(),
    PageTable::empty(),
];

/// Physical address where page tables are stored
static mut PAGE_TABLE_PHYS: u64 = 0;

/// Initialize paging
pub fn init(boot_info: &BootInfo) {
    unsafe {
        // Set up identity mapping for first 4GB using 2MB pages
        let pml4_addr = &KERNEL_PML4 as *const _ as u64;
        let pdpt_addr = &KERNEL_PDPT as *const _ as u64;
        
        // PML4[0] -> PDPT
        KERNEL_PML4.entries[0] = PageTableEntry::new(
            pdpt_addr,
            flags::PRESENT | flags::WRITABLE
        );
        
        // Higher half mapping (for kernel)
        // PML4[511] -> PDPT
        KERNEL_PML4.entries[511] = PageTableEntry::new(
            pdpt_addr,
            flags::PRESENT | flags::WRITABLE
        );
        
        // Set up PDPT entries (4 entries for 4GB)
        for i in 0..4 {
            let pd_addr = &KERNEL_PD[i] as *const _ as u64;
            KERNEL_PDPT.entries[i] = PageTableEntry::new(
                pd_addr,
                flags::PRESENT | flags::WRITABLE
            );
            
            // Set up PD entries (512 * 2MB = 1GB per PD)
            for j in 0..512 {
                let phys_addr = ((i * 512 + j) * 0x200000) as u64;
                KERNEL_PD[i].entries[j] = PageTableEntry::new(
                    phys_addr,
                    flags::PRESENT | flags::WRITABLE | flags::HUGE_PAGE
                );
            }
        }
        
        // Load new page table
        PAGE_TABLE_PHYS = pml4_addr;
        crate::arch::x86_64::write_cr3(pml4_addr);
    }
}

/// Map a virtual address to a physical address
pub fn map_page(virt: u64, phys: u64, flags: u64) -> Result<(), &'static str> {
    let indices = PageTableIndices::from_addr(virt);
    
    unsafe {
        // Get or create PDPT
        let pml4_entry = KERNEL_PML4.get_mut(indices.pml4);
        if !pml4_entry.is_present() {
            // Allocate new PDPT
            let pdpt_phys = crate::mm::physical::alloc_frame()
                .ok_or("Failed to allocate PDPT")?;
            *pml4_entry = PageTableEntry::new(pdpt_phys, flags::PRESENT | flags::WRITABLE);
            
            // Zero the new table
            let pdpt = pml4_entry.addr() as *mut PageTable;
            core::ptr::write_bytes(pdpt, 0, 1);
        }
        
        let pdpt = pml4_entry.addr() as *mut PageTable;
        let pdpt_entry = &mut (*pdpt).entries[indices.pdpt];
        
        // Get or create PD
        if !pdpt_entry.is_present() {
            let pd_phys = crate::mm::physical::alloc_frame()
                .ok_or("Failed to allocate PD")?;
            *pdpt_entry = PageTableEntry::new(pd_phys, flags::PRESENT | flags::WRITABLE);
            
            let pd = pdpt_entry.addr() as *mut PageTable;
            core::ptr::write_bytes(pd, 0, 1);
        }
        
        let pd = pdpt_entry.addr() as *mut PageTable;
        let pd_entry = &mut (*pd).entries[indices.pd];
        
        // Get or create PT
        if !pd_entry.is_present() {
            let pt_phys = crate::mm::physical::alloc_frame()
                .ok_or("Failed to allocate PT")?;
            *pd_entry = PageTableEntry::new(pt_phys, flags::PRESENT | flags::WRITABLE);
            
            let pt = pd_entry.addr() as *mut PageTable;
            core::ptr::write_bytes(pt, 0, 1);
        }
        
        // Map the page
        let pt = pd_entry.addr() as *mut PageTable;
        let pt_entry = &mut (*pt).entries[indices.pt];
        *pt_entry = PageTableEntry::new(phys, flags);
        
        // Invalidate TLB entry
        crate::arch::x86_64::invlpg(virt);
    }
    
    Ok(())
}

/// Unmap a virtual address
pub fn unmap_page(virt: u64) -> Result<u64, &'static str> {
    let indices = PageTableIndices::from_addr(virt);
    
    unsafe {
        let pml4_entry = KERNEL_PML4.get(indices.pml4);
        if !pml4_entry.is_present() {
            return Err("PML4 entry not present");
        }
        
        let pdpt = pml4_entry.addr() as *mut PageTable;
        let pdpt_entry = &(*pdpt).entries[indices.pdpt];
        if !pdpt_entry.is_present() {
            return Err("PDPT entry not present");
        }
        
        let pd = pdpt_entry.addr() as *mut PageTable;
        let pd_entry = &(*pd).entries[indices.pd];
        if !pd_entry.is_present() {
            return Err("PD entry not present");
        }
        
        let pt = pd_entry.addr() as *mut PageTable;
        let pt_entry = &mut (*pt).entries[indices.pt];
        if !pt_entry.is_present() {
            return Err("PT entry not present");
        }
        
        let phys = pt_entry.addr();
        *pt_entry = PageTableEntry::empty();
        
        crate::arch::x86_64::invlpg(virt);
        
        Ok(phys)
    }
}

/// Translate virtual address to physical address
pub fn translate(virt: u64) -> Option<u64> {
    let indices = PageTableIndices::from_addr(virt);
    
    unsafe {
        let pml4_entry = KERNEL_PML4.get(indices.pml4);
        if !pml4_entry.is_present() {
            return None;
        }
        
        let pdpt = pml4_entry.addr() as *const PageTable;
        let pdpt_entry = &(*pdpt).entries[indices.pdpt];
        if !pdpt_entry.is_present() {
            return None;
        }
        
        // Check for 1GB page
        if pdpt_entry.is_huge() {
            let phys = pdpt_entry.addr() + (virt & 0x3FFF_FFFF);
            return Some(phys);
        }
        
        let pd = pdpt_entry.addr() as *const PageTable;
        let pd_entry = &(*pd).entries[indices.pd];
        if !pd_entry.is_present() {
            return None;
        }
        
        // Check for 2MB page
        if pd_entry.is_huge() {
            let phys = pd_entry.addr() + (virt & 0x1F_FFFF);
            return Some(phys);
        }
        
        let pt = pd_entry.addr() as *const PageTable;
        let pt_entry = &(*pt).entries[indices.pt];
        if !pt_entry.is_present() {
            return None;
        }
        
        Some(pt_entry.addr() + indices.offset as u64)
    }
}
