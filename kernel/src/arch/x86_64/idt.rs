//! Interrupt Descriptor Table (IDT) for x86_64

use crate::arch::x86_64::gdt::KERNEL_CODE_SELECTOR;
use core::mem::size_of;

/// IDT entry type
#[derive(Clone, Copy)]
#[repr(u8)]
pub enum GateType {
    Interrupt = 0xE,
    Trap = 0xF,
}

/// IDT entry structure
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    type_attr: u8,
    offset_mid: u16,
    offset_high: u32,
    reserved: u32,
}

impl IdtEntry {
    const fn null() -> Self {
        Self {
            offset_low: 0,
            selector: 0,
            ist: 0,
            type_attr: 0,
            offset_mid: 0,
            offset_high: 0,
            reserved: 0,
        }
    }
    
    fn new(handler: u64, selector: u16, ist: u8, gate_type: GateType, dpl: u8) -> Self {
        Self {
            offset_low: (handler & 0xFFFF) as u16,
            selector,
            ist,
            type_attr: (1 << 7) | ((dpl & 3) << 5) | (gate_type as u8),
            offset_mid: ((handler >> 16) & 0xFFFF) as u16,
            offset_high: ((handler >> 32) & 0xFFFFFFFF) as u32,
            reserved: 0,
        }
    }
    
    fn set_handler(&mut self, handler: u64) {
        self.offset_low = (handler & 0xFFFF) as u16;
        self.offset_mid = ((handler >> 16) & 0xFFFF) as u16;
        self.offset_high = ((handler >> 32) & 0xFFFFFFFF) as u32;
        self.selector = KERNEL_CODE_SELECTOR;
        self.type_attr = (1 << 7) | GateType::Interrupt as u8;
    }
}

/// IDT descriptor
#[repr(C, packed)]
struct IdtDescriptor {
    size: u16,
    offset: u64,
}

/// IDT structure (256 entries)
#[repr(C, align(16))]
struct Idt {
    entries: [IdtEntry; 256],
}

/// Global IDT instance
static mut IDT: Idt = Idt {
    entries: [IdtEntry::null(); 256],
};

/// Initialize IDT
pub fn init() {
    unsafe {
        // CPU exceptions (0-31)
        IDT.entries[0].set_handler(divide_error as u64);
        IDT.entries[1].set_handler(debug as u64);
        IDT.entries[2].set_handler(nmi as u64);
        IDT.entries[3].set_handler(breakpoint as u64);
        IDT.entries[4].set_handler(overflow as u64);
        IDT.entries[5].set_handler(bound_range as u64);
        IDT.entries[6].set_handler(invalid_opcode as u64);
        IDT.entries[7].set_handler(device_not_available as u64);
        IDT.entries[8] = IdtEntry::new(double_fault as u64, KERNEL_CODE_SELECTOR, 1, GateType::Interrupt, 0);
        IDT.entries[10].set_handler(invalid_tss as u64);
        IDT.entries[11].set_handler(segment_not_present as u64);
        IDT.entries[12].set_handler(stack_segment as u64);
        IDT.entries[13].set_handler(general_protection as u64);
        IDT.entries[14].set_handler(page_fault as u64);
        IDT.entries[16].set_handler(x87_fp_exception as u64);
        IDT.entries[17].set_handler(alignment_check as u64);
        IDT.entries[18].set_handler(machine_check as u64);
        IDT.entries[19].set_handler(simd_fp_exception as u64);
        IDT.entries[20].set_handler(virtualization as u64);
        
        // IRQs (32-47)
        IDT.entries[32].set_handler(irq0 as u64);  // Timer
        IDT.entries[33].set_handler(irq1 as u64);  // Keyboard
        IDT.entries[34].set_handler(irq2 as u64);
        IDT.entries[35].set_handler(irq3 as u64);
        IDT.entries[36].set_handler(irq4 as u64);
        IDT.entries[37].set_handler(irq5 as u64);
        IDT.entries[38].set_handler(irq6 as u64);
        IDT.entries[39].set_handler(irq7 as u64);
        IDT.entries[40].set_handler(irq8 as u64);
        IDT.entries[41].set_handler(irq9 as u64);
        IDT.entries[42].set_handler(irq10 as u64);
        IDT.entries[43].set_handler(irq11 as u64);
        IDT.entries[44].set_handler(irq12 as u64);
        IDT.entries[45].set_handler(irq13 as u64);
        IDT.entries[46].set_handler(irq14 as u64);
        IDT.entries[47].set_handler(irq15 as u64);
        
        // Syscall interrupt
        IDT.entries[0x80] = IdtEntry::new(syscall_handler as u64, KERNEL_CODE_SELECTOR, 0, GateType::Trap, 3);
        
        // Load IDT
        let idt_descriptor = IdtDescriptor {
            size: (size_of::<Idt>() - 1) as u16,
            offset: &IDT as *const _ as u64,
        };
        
        core::arch::asm!(
            "lidt [{}]",
            in(reg) &idt_descriptor,
            options(nostack)
        );
    }
    
    // Initialize PIC
    init_pic();
}

/// Initialize PIC (Programmable Interrupt Controller)
fn init_pic() {
    use crate::arch::x86_64::{outb, inb};
    
    const PIC1_CMD: u16 = 0x20;
    const PIC1_DATA: u16 = 0x21;
    const PIC2_CMD: u16 = 0xA0;
    const PIC2_DATA: u16 = 0xA1;
    
    // Save masks
    let _mask1 = inb(PIC1_DATA);
    let _mask2 = inb(PIC2_DATA);
    
    // ICW1: Initialize + ICW4 needed
    outb(PIC1_CMD, 0x11);
    outb(PIC2_CMD, 0x11);
    
    // ICW2: Vector offset
    outb(PIC1_DATA, 0x20); // IRQs 0-7 -> interrupts 32-39
    outb(PIC2_DATA, 0x28); // IRQs 8-15 -> interrupts 40-47
    
    // ICW3: Cascade identity
    outb(PIC1_DATA, 0x04); // IRQ2 has slave
    outb(PIC2_DATA, 0x02); // Slave identity
    
    // ICW4: 8086 mode
    outb(PIC1_DATA, 0x01);
    outb(PIC2_DATA, 0x01);
    
    // Restore masks (enable all for now)
    outb(PIC1_DATA, 0x00);
    outb(PIC2_DATA, 0x00);
}

/// Send EOI to PIC
pub fn send_eoi(irq: u8) {
    use crate::arch::x86_64::outb;
    
    const PIC1_CMD: u16 = 0x20;
    const PIC2_CMD: u16 = 0xA0;
    
    if irq >= 8 {
        outb(PIC2_CMD, 0x20);
    }
    outb(PIC1_CMD, 0x20);
}

/// Interrupt stack frame
#[repr(C)]
pub struct InterruptStackFrame {
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

// Exception handler inner functions
extern "C" fn divide_error_handler(_frame: *const u64) {
    crate::kprintln!("Exception: Division Error");
}

extern "C" fn debug_handler(_frame: *const u64) {
    crate::kprintln!("Exception: Debug");
}

extern "C" fn nmi_handler(_frame: *const u64) {
    crate::kprintln!("Exception: Non-Maskable Interrupt");
}

extern "C" fn breakpoint_handler(_frame: *const u64) {
    crate::kprintln!("Exception: Breakpoint");
}

extern "C" fn overflow_handler(_frame: *const u64) {
    crate::kprintln!("Exception: Overflow");
}

extern "C" fn bound_range_handler(_frame: *const u64) {
    crate::kprintln!("Exception: Bound Range Exceeded");
}

extern "C" fn invalid_opcode_handler(_frame: *const u64) {
    crate::kprintln!("Exception: Invalid Opcode");
}

extern "C" fn device_not_available_handler(_frame: *const u64) {
    crate::kprintln!("Exception: Device Not Available");
}

extern "C" fn invalid_tss_handler(_frame: *const u64) {
    crate::kprintln!("Exception: Invalid TSS");
}

extern "C" fn segment_not_present_handler(_frame: *const u64) {
    crate::kprintln!("Exception: Segment Not Present");
}

extern "C" fn stack_segment_handler(_frame: *const u64) {
    crate::kprintln!("Exception: Stack-Segment Fault");
}

extern "C" fn general_protection_handler(_frame: *const u64) {
    crate::kprintln!("Exception: General Protection Fault");
}

extern "C" fn x87_fp_exception_handler(_frame: *const u64) {
    crate::kprintln!("Exception: x87 Floating-Point Exception");
}

extern "C" fn alignment_check_handler(_frame: *const u64) {
    crate::kprintln!("Exception: Alignment Check");
}

extern "C" fn machine_check_handler(_frame: *const u64) {
    crate::kprintln!("Exception: Machine Check");
}

extern "C" fn simd_fp_exception_handler(_frame: *const u64) {
    crate::kprintln!("Exception: SIMD Floating-Point Exception");
}

extern "C" fn virtualization_handler(_frame: *const u64) {
    crate::kprintln!("Exception: Virtualization Exception");
}

// Macro for creating exception handlers without error code
macro_rules! exception_handler_no_error {
    ($name:ident, $handler:ident) => {
        #[unsafe(naked)]
        extern "C" fn $name() {
            core::arch::naked_asm!(
                "push rax",
                "push rbx",
                "push rcx",
                "push rdx",
                "push rsi",
                "push rdi",
                "push rbp",
                "push r8",
                "push r9",
                "push r10",
                "push r11",
                "push r12",
                "push r13",
                "push r14",
                "push r15",
                "mov rdi, rsp",
                "call {handler}",
                "pop r15",
                "pop r14",
                "pop r13",
                "pop r12",
                "pop r11",
                "pop r10",
                "pop r9",
                "pop r8",
                "pop rbp",
                "pop rdi",
                "pop rsi",
                "pop rdx",
                "pop rcx",
                "pop rbx",
                "pop rax",
                "iretq",
                handler = sym $handler,
            );
        }
    };
}

// Macro for creating exception handlers with error code
macro_rules! exception_handler_with_error {
    ($name:ident, $handler:ident) => {
        #[unsafe(naked)]
        extern "C" fn $name() {
            core::arch::naked_asm!(
                "push rax",
                "push rbx",
                "push rcx",
                "push rdx",
                "push rsi",
                "push rdi",
                "push rbp",
                "push r8",
                "push r9",
                "push r10",
                "push r11",
                "push r12",
                "push r13",
                "push r14",
                "push r15",
                "mov rdi, rsp",
                "call {handler}",
                "pop r15",
                "pop r14",
                "pop r13",
                "pop r12",
                "pop r11",
                "pop r10",
                "pop r9",
                "pop r8",
                "pop rbp",
                "pop rdi",
                "pop rsi",
                "pop rdx",
                "pop rcx",
                "pop rbx",
                "pop rax",
                "add rsp, 8",
                "iretq",
                handler = sym $handler,
            );
        }
    };
}

// Generate exception handlers
exception_handler_no_error!(divide_error, divide_error_handler);
exception_handler_no_error!(debug, debug_handler);
exception_handler_no_error!(nmi, nmi_handler);
exception_handler_no_error!(breakpoint, breakpoint_handler);
exception_handler_no_error!(overflow, overflow_handler);
exception_handler_no_error!(bound_range, bound_range_handler);
exception_handler_no_error!(invalid_opcode, invalid_opcode_handler);
exception_handler_no_error!(device_not_available, device_not_available_handler);
exception_handler_no_error!(x87_fp_exception, x87_fp_exception_handler);
exception_handler_no_error!(machine_check, machine_check_handler);
exception_handler_no_error!(simd_fp_exception, simd_fp_exception_handler);
exception_handler_no_error!(virtualization, virtualization_handler);

exception_handler_with_error!(invalid_tss, invalid_tss_handler);
exception_handler_with_error!(segment_not_present, segment_not_present_handler);
exception_handler_with_error!(stack_segment, stack_segment_handler);
exception_handler_with_error!(general_protection, general_protection_handler);
exception_handler_with_error!(alignment_check, alignment_check_handler);

/// Double fault handler (uses IST)
#[unsafe(naked)]
extern "C" fn double_fault() {
    core::arch::naked_asm!(
        "mov rdi, rsp",
        "call {handler}",
        "iretq",
        handler = sym double_fault_inner,
    );
}

extern "C" fn double_fault_inner(_frame: *const u64) -> ! {
    crate::kprintln!("DOUBLE FAULT!");
    loop {
        crate::arch::halt();
    }
}

/// Page fault handler
#[unsafe(naked)]
extern "C" fn page_fault() {
    core::arch::naked_asm!(
        "push rax",
        "push rbx",
        "push rcx",
        "push rdx",
        "push rsi",
        "push rdi",
        "push rbp",
        "push r8",
        "push r9",
        "push r10",
        "push r11",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        "mov rdi, rsp",
        "call {handler}",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rbp",
        "pop rdi",
        "pop rsi",
        "pop rdx",
        "pop rcx",
        "pop rbx",
        "pop rax",
        "add rsp, 8",
        "iretq",
        handler = sym page_fault_inner,
    );
}

extern "C" fn page_fault_inner(_frame: *const u64) {
    let cr2 = crate::arch::x86_64::read_cr2();
    crate::kprintln!("Page Fault at address: {:#x}", cr2);
}

// IRQ handlers
extern "C" fn irq_common_handler(irq: u8) {
    match irq {
        0 => crate::proc::scheduler::timer_tick(),
        1 => crate::drivers::keyboard::handle_interrupt(),
        12 => crate::drivers::mouse::handle_interrupt(),
        _ => {}
    }
    send_eoi(irq);
}

macro_rules! irq_handler {
    ($name:ident, $irq:expr) => {
        #[unsafe(naked)]
        extern "C" fn $name() {
            core::arch::naked_asm!(
                "push rax",
                "push rbx",
                "push rcx",
                "push rdx",
                "push rsi",
                "push rdi",
                "push rbp",
                "push r8",
                "push r9",
                "push r10",
                "push r11",
                "push r12",
                "push r13",
                "push r14",
                "push r15",
                "mov rdi, {irq}",
                "call {handler}",
                "pop r15",
                "pop r14",
                "pop r13",
                "pop r12",
                "pop r11",
                "pop r10",
                "pop r9",
                "pop r8",
                "pop rbp",
                "pop rdi",
                "pop rsi",
                "pop rdx",
                "pop rcx",
                "pop rbx",
                "pop rax",
                "iretq",
                irq = const $irq,
                handler = sym irq_common_handler,
            );
        }
    };
}

irq_handler!(irq0, 0);
irq_handler!(irq1, 1);
irq_handler!(irq2, 2);
irq_handler!(irq3, 3);
irq_handler!(irq4, 4);
irq_handler!(irq5, 5);
irq_handler!(irq6, 6);
irq_handler!(irq7, 7);
irq_handler!(irq8, 8);
irq_handler!(irq9, 9);
irq_handler!(irq10, 10);
irq_handler!(irq11, 11);
irq_handler!(irq12, 12);
irq_handler!(irq13, 13);
irq_handler!(irq14, 14);
irq_handler!(irq15, 15);

/// Syscall handler
#[unsafe(naked)]
extern "C" fn syscall_handler() {
    core::arch::naked_asm!(
        "push rax",
        "push rbx",
        "push rcx",
        "push rdx",
        "push rsi",
        "push rdi",
        "push rbp",
        "push r8",
        "push r9",
        "push r10",
        "push r11",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        "mov rdi, rsp",
        "call {handler}",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rbp",
        "pop rdi",
        "pop rsi",
        "pop rdx",
        "pop rcx",
        "pop rbx",
        "pop rax",
        "iretq",
        handler = sym syscall_handler_inner,
    );
}

extern "C" fn syscall_handler_inner(frame: *const u64) {
    unsafe {
        let regs = frame as *const [u64; 15];
        let syscall_num = (*regs)[14] as usize; // rax
        let arg1 = (*regs)[9] as usize;         // rdi
        let arg2 = (*regs)[8] as usize;         // rsi
        let arg3 = (*regs)[7] as usize;         // rdx
        let arg4 = (*regs)[6] as usize;         // r10
        let arg5 = (*regs)[5] as usize;         // r8
        
        let _result = crate::syscall::handle(syscall_num, arg1, arg2, arg3, arg4, arg5);
    }
}
