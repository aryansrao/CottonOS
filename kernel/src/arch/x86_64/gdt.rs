//! Global Descriptor Table (GDT) for x86_64

use core::mem::size_of;

/// GDT entry structure
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct GdtEntry {
    limit_low: u16,
    base_low: u16,
    base_middle: u8,
    access: u8,
    granularity: u8,
    base_high: u8,
}

impl GdtEntry {
    const fn null() -> Self {
        Self {
            limit_low: 0,
            base_low: 0,
            base_middle: 0,
            access: 0,
            granularity: 0,
            base_high: 0,
        }
    }
    
    const fn new(base: u32, limit: u32, access: u8, granularity: u8) -> Self {
        Self {
            limit_low: (limit & 0xFFFF) as u16,
            base_low: (base & 0xFFFF) as u16,
            base_middle: ((base >> 16) & 0xFF) as u8,
            access,
            granularity: ((limit >> 16) & 0x0F) as u8 | (granularity & 0xF0),
            base_high: ((base >> 24) & 0xFF) as u8,
        }
    }
    
    const fn code_segment() -> Self {
        // Present, ring 0, code, readable, long mode
        Self::new(0, 0xFFFFF, 0x9A, 0xA0)
    }
    
    const fn data_segment() -> Self {
        // Present, ring 0, data, writable
        Self::new(0, 0xFFFFF, 0x92, 0xC0)
    }
    
    const fn user_code_segment() -> Self {
        // Present, ring 3, code, readable, long mode
        Self::new(0, 0xFFFFF, 0xFA, 0xA0)
    }
    
    const fn user_data_segment() -> Self {
        // Present, ring 3, data, writable
        Self::new(0, 0xFFFFF, 0xF2, 0xC0)
    }
}

/// TSS entry (16 bytes for x86_64)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct TssEntry {
    length: u16,
    base_low: u16,
    base_middle: u8,
    flags1: u8,
    flags2: u8,
    base_high: u8,
    base_upper: u32,
    reserved: u32,
}

impl TssEntry {
    const fn null() -> Self {
        Self {
            length: 0,
            base_low: 0,
            base_middle: 0,
            flags1: 0,
            flags2: 0,
            base_high: 0,
            base_upper: 0,
            reserved: 0,
        }
    }
    
    fn new(tss_addr: u64, tss_size: u16) -> Self {
        Self {
            length: tss_size,
            base_low: (tss_addr & 0xFFFF) as u16,
            base_middle: ((tss_addr >> 16) & 0xFF) as u8,
            flags1: 0x89, // Present, available TSS
            flags2: 0,
            base_high: ((tss_addr >> 24) & 0xFF) as u8,
            base_upper: ((tss_addr >> 32) & 0xFFFFFFFF) as u32,
            reserved: 0,
        }
    }
}

/// Task State Segment
#[repr(C, packed)]
pub struct TaskStateSegment {
    reserved1: u32,
    pub rsp0: u64,
    pub rsp1: u64,
    pub rsp2: u64,
    reserved2: u64,
    pub ist1: u64,
    pub ist2: u64,
    pub ist3: u64,
    pub ist4: u64,
    pub ist5: u64,
    pub ist6: u64,
    pub ist7: u64,
    reserved3: u64,
    reserved4: u16,
    pub iomap_base: u16,
}

impl TaskStateSegment {
    pub const fn new() -> Self {
        Self {
            reserved1: 0,
            rsp0: 0,
            rsp1: 0,
            rsp2: 0,
            reserved2: 0,
            ist1: 0,
            ist2: 0,
            ist3: 0,
            ist4: 0,
            ist5: 0,
            ist6: 0,
            ist7: 0,
            reserved3: 0,
            reserved4: 0,
            iomap_base: size_of::<TaskStateSegment>() as u16,
        }
    }
}

/// GDT descriptor
#[repr(C, packed)]
struct GdtDescriptor {
    size: u16,
    offset: u64,
}

/// GDT structure
#[repr(C, packed)]
struct Gdt {
    null: GdtEntry,
    kernel_code: GdtEntry,
    kernel_data: GdtEntry,
    user_code: GdtEntry,
    user_data: GdtEntry,
    tss: TssEntry,
}

/// Global GDT instance
static mut GDT: Gdt = Gdt {
    null: GdtEntry::null(),
    kernel_code: GdtEntry::code_segment(),
    kernel_data: GdtEntry::data_segment(),
    user_code: GdtEntry::user_code_segment(),
    user_data: GdtEntry::user_data_segment(),
    tss: TssEntry::null(),
};

/// Global TSS instance
static mut TSS: TaskStateSegment = TaskStateSegment::new();

/// Kernel stack for syscalls and interrupts
static mut KERNEL_STACK: [u8; 32768] = [0; 32768];
static mut IST_STACK1: [u8; 16384] = [0; 16384];

/// Segment selectors
pub const KERNEL_CODE_SELECTOR: u16 = 0x08;
pub const KERNEL_DATA_SELECTOR: u16 = 0x10;
pub const USER_CODE_SELECTOR: u16 = 0x18 | 3;
pub const USER_DATA_SELECTOR: u16 = 0x20 | 3;
pub const TSS_SELECTOR: u16 = 0x28;

/// Initialize GDT
pub fn init() {
    unsafe {
        // Set up TSS
        let tss_addr = &TSS as *const _ as u64;
        let tss_size = (size_of::<TaskStateSegment>() - 1) as u16;
        
        // Set kernel stack pointer
        TSS.rsp0 = (&KERNEL_STACK as *const _ as u64) + KERNEL_STACK.len() as u64;
        TSS.ist1 = (&IST_STACK1 as *const _ as u64) + IST_STACK1.len() as u64;
        
        // Set TSS entry in GDT
        GDT.tss = TssEntry::new(tss_addr, tss_size);
        
        // Create GDT descriptor
        let gdt_descriptor = GdtDescriptor {
            size: (size_of::<Gdt>() - 1) as u16,
            offset: &GDT as *const _ as u64,
        };
        
        // Load GDT
        load_gdt(&gdt_descriptor);
        
        // Reload segment registers
        reload_segments();
        
        // Load TSS
        load_tss(TSS_SELECTOR);
    }
}

/// Load GDT using lgdt instruction
unsafe fn load_gdt(descriptor: &GdtDescriptor) {
    core::arch::asm!(
        "lgdt [{}]",
        in(reg) descriptor,
        options(nostack)
    );
}

/// Reload segment registers
unsafe fn reload_segments() {
    core::arch::asm!(
        "push {0}",          // Push code selector
        "lea {1}, [rip + 2f]", // Get address of label
        "push {1}",          // Push return address
        "retfq",             // Far return
        "2:",
        "mov ds, {2:x}",     // Reload data segments
        "mov es, {2:x}",
        "mov fs, {2:x}",
        "mov gs, {2:x}",
        "mov ss, {2:x}",
        in(reg) KERNEL_CODE_SELECTOR as u64,
        lateout(reg) _,
        in(reg) KERNEL_DATA_SELECTOR as u32,
        options(nostack)
    );
}

/// Load TSS
unsafe fn load_tss(selector: u16) {
    core::arch::asm!(
        "ltr {0:x}",
        in(reg) selector,
        options(nostack)
    );
}

/// Get TSS mutable reference
pub fn get_tss() -> &'static mut TaskStateSegment {
    unsafe { &mut TSS }
}
