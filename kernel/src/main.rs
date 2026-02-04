//! CottonOS Kernel
//! 
//! A modern operating system kernel for x86_64.

#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]
#![allow(dead_code)]
#![allow(static_mut_refs)]           // Kernel needs mutable statics for low-level hardware access
#![allow(unused_variables)]          // Many syscall/driver stubs have unused parameters

extern crate alloc;

// Module declarations
pub mod arch;
pub mod mm;
pub mod proc;
pub mod fs;
pub mod drivers;
pub mod syscall;
pub mod sync;
pub mod shell;
pub mod gui;

use core::panic::PanicInfo;
use core::sync::atomic::{AtomicBool, Ordering};

// Note: Multiboot header, page tables, GDT are all in boot_stub.asm

/// Early serial output for debugging
#[cfg(target_arch = "x86_64")]
fn early_serial_write(s: &[u8]) {
    const COM1: u16 = 0x3F8;
    for &byte in s {
        // Wait for transmit buffer empty
        unsafe {
            loop {
                let status: u8;
                core::arch::asm!(
                    "in al, dx",
                    out("al") status,
                    in("dx") COM1 + 5,
                    options(nomem, nostack)
                );
                if status & 0x20 != 0 { break; }
            }
            // Write byte
            core::arch::asm!(
                "out dx, al",
                in("dx") COM1,
                in("al") byte,
                options(nomem, nostack)
            );
        }
    }
}

/// Initialize serial port early (before full kernel init)
#[cfg(target_arch = "x86_64")]
fn early_serial_init() {
    const COM1: u16 = 0x3F8;
    unsafe {
        // Helper to output byte
        macro_rules! outb {
            ($port:expr, $val:expr) => {
                core::arch::asm!(
                    "out dx, al",
                    in("dx") $port as u16,
                    in("al") $val as u8,
                    options(nomem, nostack)
                );
            };
        }
        // Disable interrupts
        outb!(COM1 + 1, 0x00);
        // Enable DLAB
        outb!(COM1 + 3, 0x80);
        // Set baud rate divisor to 1 (115200 baud)
        outb!(COM1 + 0, 0x01);
        outb!(COM1 + 1, 0x00);
        // 8 bits, no parity, one stop bit
        outb!(COM1 + 3, 0x03);
        // Enable FIFO
        outb!(COM1 + 2, 0xC7);
        // IRQs enabled, RTS/DSR set
        outb!(COM1 + 4, 0x0F);
    }
}

/// Entry point for x86_64 - called from assembly boot code
/// This is called after we're already in 64-bit long mode
#[cfg(target_arch = "x86_64")]
#[no_mangle]
pub extern "C" fn _start64(multiboot_info: u64) -> ! {
    // Initialize serial first for debugging
    early_serial_init();
    early_serial_write(b"=== CottonOS Rust Entry ===\r\n");
    
    // Default VGA text mode values
    let mut framebuffer_addr: u64 = 0xb8000;
    let mut framebuffer_width: u32 = 80;
    let mut framebuffer_height: u32 = 25;
    let mut framebuffer_pitch: u32 = 160;
    let mut framebuffer_bpp: u8 = 16;
    
    if multiboot_info != 0 {
        early_serial_write(b"Parsing Multiboot2 info...\r\n");
        
        // Multiboot2 structure format:
        // u32 total_size
        // u32 reserved (must be 0)
        // ... tags ...
        // Tag format:
        //   u32 type
        //   u32 size
        //   ... data (size - 8 bytes) ...
        //   padding to 8-byte alignment
        
        unsafe {
            let total_size = *(multiboot_info as *const u32);
            early_serial_write(b"Total size: ");
            
            let mut addr = multiboot_info + 8; // Skip total_size and reserved
            let end = multiboot_info + total_size as u64;
            
            while addr < end {
                let tag_type = *(addr as *const u32);
                let tag_size = *((addr + 4) as *const u32);
                
                if tag_type == 0 {
                    // End tag
                    break;
                }
                
                // Framebuffer info tag (type 8)
                if tag_type == 8 {
                    early_serial_write(b"Found framebuffer tag!\r\n");
                    
                    // Framebuffer tag format:
                    // u32 type (8)
                    // u32 size
                    // u64 framebuffer_addr
                    // u32 framebuffer_pitch
                    // u32 framebuffer_width
                    // u32 framebuffer_height
                    // u8  framebuffer_bpp
                    // u8  framebuffer_type
                    // u16 reserved
                    // ... color info ...
                    
                    framebuffer_addr = *((addr + 8) as *const u64);
                    framebuffer_pitch = *((addr + 16) as *const u32);
                    framebuffer_width = *((addr + 20) as *const u32);
                    framebuffer_height = *((addr + 24) as *const u32);
                    framebuffer_bpp = *((addr + 28) as *const u8);
                    let fb_type = *((addr + 29) as *const u8);
                    
                    // Debug: print framebuffer info
                    early_serial_write(b"FB: ");
                    // Print width as decimal
                    let mut w = framebuffer_width;
                    let mut buf = [0u8; 10];
                    let mut i = 9;
                    loop {
                        buf[i] = b'0' + (w % 10) as u8;
                        w /= 10;
                        if w == 0 { break; }
                        i -= 1;
                    }
                    early_serial_write(&buf[i..]);
                    early_serial_write(b"x");
                    // Print height
                    let mut h = framebuffer_height;
                    i = 9;
                    loop {
                        buf[i] = b'0' + (h % 10) as u8;
                        h /= 10;
                        if h == 0 { break; }
                        i -= 1;
                    }
                    early_serial_write(&buf[i..]);
                    early_serial_write(b"x");
                    // Print bpp
                    let mut b = framebuffer_bpp as u32;
                    i = 9;
                    loop {
                        buf[i] = b'0' + (b % 10) as u8;
                        b /= 10;
                        if b == 0 { break; }
                        i -= 1;
                    }
                    early_serial_write(&buf[i..]);
                    early_serial_write(b" type=");
                    buf[9] = b'0' + fb_type;
                    early_serial_write(&buf[9..]);
                    early_serial_write(b"\r\n");
                    
                    // Multiboot2 framebuffer types:
                    // 0 = indexed color (palette)
                    // 1 = direct RGB color (what we want!)
                    // 2 = EGA text mode
                    if fb_type == 1 && framebuffer_addr != 0 && framebuffer_width > 0 && framebuffer_bpp >= 24 {
                        early_serial_write(b"Using RGB framebuffer\r\n");
                    } else if fb_type == 0 && framebuffer_addr != 0 {
                        early_serial_write(b"Using indexed color framebuffer\r\n");
                    } else if fb_type == 2 {
                        early_serial_write(b"EGA text mode framebuffer\r\n");
                        framebuffer_addr = 0xb8000;
                        framebuffer_width = 80;
                        framebuffer_height = 25;
                        framebuffer_pitch = 160;
                        framebuffer_bpp = 16;
                    } else {
                        early_serial_write(b"Invalid framebuffer, using VGA\r\n");
                        framebuffer_addr = 0xb8000;
                        framebuffer_width = 80;
                        framebuffer_height = 25;
                        framebuffer_pitch = 160;
                        framebuffer_bpp = 16;
                    }
                }
                
                // Move to next tag (align to 8 bytes)
                addr += ((tag_size + 7) & !7) as u64;
            }
            
            if framebuffer_addr == 0xb8000 {
                early_serial_write(b"No framebuffer found, using VGA text mode\r\n");
            }
        }
    } else {
        early_serial_write(b"No multiboot info, using VGA text mode\r\n");
    }
    
    early_serial_write(b"Creating boot info...\r\n");
    
    // Create boot info with framebuffer
    let boot_info = BootInfo {
        magic: multiboot_info,
        memory_map: core::ptr::null(),
        memory_map_entries: 0,
        framebuffer: FramebufferInfo {
            address: framebuffer_addr,
            width: framebuffer_width,
            height: framebuffer_height,
            pitch: framebuffer_pitch,
            bpp: framebuffer_bpp,
            red_shift: 16,
            green_shift: 8,
            blue_shift: 0,
        },
        arch: Architecture::X86_64,
        kernel_start: 0x100000,
        kernel_end: 0x200000,
        initrd_start: 0,
        initrd_end: 0,
        cmdline: core::ptr::null(),
        cmdline_len: 0,
    };
    
    early_serial_write(b"Calling kernel_main...\r\n");
    kernel_main(&boot_info)
}


/// Rust entry point - receives boot info pointer
#[no_mangle]
pub extern "C" fn _rust_start(boot_info_ptr: u64) -> ! {
    // Initialize serial EARLY for debugging
    #[cfg(target_arch = "x86_64")]
    {
        early_serial_init();
        early_serial_write(b"\r\n=== CottonOS Boot ===\r\n");
    }
    
    // Create a minimal boot info if pointer is null/invalid
    if boot_info_ptr == 0 {
        #[cfg(target_arch = "x86_64")]
        early_serial_write(b"Using default boot info\r\n");
        
        // Create default boot info on stack for direct QEMU boot
        let boot_info = BootInfo {
            magic: 0,
            memory_map: core::ptr::null(),
            memory_map_entries: 0,
            framebuffer: FramebufferInfo {
                address: 0xb8000,
                width: 80,
                height: 25,
                pitch: 160,
                bpp: 16,
                red_shift: 0,
                green_shift: 0,
                blue_shift: 0,
            },
            arch: Architecture::X86_64,
            kernel_start: 0x100000,
            kernel_end: 0x200000,
            initrd_start: 0,
            initrd_end: 0,
            cmdline: core::ptr::null(),
            cmdline_len: 0,
        };
        kernel_main(&boot_info)
    } else {
        #[cfg(target_arch = "x86_64")]
        early_serial_write(b"Boot info from bootloader\r\n");
        
        // We pass a reference, so this is safe
        let boot_info = unsafe { &*(boot_info_ptr as *const BootInfo) };
        kernel_main(boot_info)
    }
}

/// Kernel version information
pub const KERNEL_VERSION: &str = "0.1.0";
pub const KERNEL_NAME: &str = "CottonOS";

/// Boot information structure passed from bootloader
#[repr(C)]
pub struct BootInfo {
    pub magic: u64,
    pub memory_map: *const mm::MemoryMapEntry,
    pub memory_map_entries: usize,
    pub framebuffer: FramebufferInfo,
    pub arch: Architecture,
    pub kernel_start: u64,
    pub kernel_end: u64,
    pub initrd_start: u64,
    pub initrd_end: u64,
    pub cmdline: *const u8,
    pub cmdline_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FramebufferInfo {
    pub address: u64,
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    pub bpp: u8,
    pub red_shift: u8,
    pub green_shift: u8,
    pub blue_shift: u8,
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Architecture {
    X86 = 0,
    X86_64 = 1,
    Arm32 = 2,
    Arm64 = 3,
    Unknown = 255,
}

impl Architecture {
    /// Get current architecture at compile time
    pub fn current() -> Self {
        #[cfg(target_arch = "x86_64")]
        { Architecture::X86_64 }
        #[cfg(target_arch = "x86")]
        { Architecture::X86 }
        #[cfg(target_arch = "aarch64")]
        { Architecture::Arm64 }
        #[cfg(target_arch = "arm")]
        { Architecture::Arm32 }
        #[cfg(not(any(target_arch = "x86_64", target_arch = "x86", target_arch = "aarch64", target_arch = "arm")))]
        { Architecture::Unknown }
    }
}

/// Static flag to track if kernel has been initialized
static KERNEL_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Kernel main entry point
/// Called by architecture-specific entry code
#[no_mangle]
pub extern "C" fn kernel_main(boot_info: *const BootInfo) -> ! {
    // Debug output before anything else
    #[cfg(target_arch = "x86_64")]
    early_serial_write(b"kernel_main entered\r\n");
    
    // Prevent re-initialization
    if KERNEL_INITIALIZED.swap(true, Ordering::SeqCst) {
        #[cfg(target_arch = "x86_64")]
        early_serial_write(b"PANIC: kernel already initialized!\r\n");
        loop {
            arch::halt();
        }
    }
    
    #[cfg(target_arch = "x86_64")]
    early_serial_write(b"Initializing arch...\r\n");

    let boot_info = unsafe { &*boot_info };
    
    // Initialize architecture-specific components
    arch::init(boot_info);
    
    // Print boot message
    kprintln!("");
    kprintln!("+==========================================================+");
    kprintln!("|                      CottonOS v{}                       |", KERNEL_VERSION);
    kprintln!("|           A Modern Multi-Architecture OS Kernel          |");
    kprintln!("+==========================================================+");
    kprintln!("");
    
    // Detect and display architecture
    kprintln!("[BOOT] Architecture: {:?}", boot_info.arch);
    kprintln!("[BOOT] Kernel loaded at: {:#x} - {:#x}", 
              boot_info.kernel_start, boot_info.kernel_end);
    
    // Initialize memory management
    kprintln!("[INIT] Setting up memory management...");
    mm::init(boot_info);
    kprintln!("[INIT] Memory management initialized");
    
    // Initialize process management
    kprintln!("[INIT] Setting up process management...");
    proc::init();
    kprintln!("[INIT] Process management initialized");
    
    // Initialize device drivers
    kprintln!("[INIT] Setting up device drivers...");
    drivers::init();
    kprintln!("[INIT] Device drivers initialized");

    // Initialize filesystem
    kprintln!("[INIT] Setting up filesystem...");
    fs::init();
    kprintln!("[INIT] Filesystem initialized");
    
    kprintln!("[INIT] Filesystem initialized");
    
    // Debug framebuffer info
    kprintln!("[DEBUG] FB check: addr={:#x} w={} h={} bpp={}",
        boot_info.framebuffer.address,
        boot_info.framebuffer.width,
        boot_info.framebuffer.height,
        boot_info.framebuffer.bpp);
    
    // Initialize graphics if framebuffer is available
    // Accept any framebuffer that's not VGA text mode (0xb8000) and is at least 640x480
    if boot_info.framebuffer.address != 0xb8000 && 
       boot_info.framebuffer.width >= 640 && 
       boot_info.framebuffer.height >= 480 &&
       boot_info.framebuffer.bpp >= 8 {
        kprintln!("[INIT] Framebuffer: {}x{} @ {:#x} ({}bpp)", 
            boot_info.framebuffer.width,
            boot_info.framebuffer.height,
            boot_info.framebuffer.address,
            boot_info.framebuffer.bpp);
        
        drivers::init_graphics(
            boot_info.framebuffer.address,
            boot_info.framebuffer.width,
            boot_info.framebuffer.height,
            boot_info.framebuffer.pitch,
            boot_info.framebuffer.bpp
        );
        
        // Initialize GUI
        kprintln!("[INIT] Initializing GUI...");
        gui::init();
    } else {
        kprintln!("[INIT] No framebuffer, running in text mode");
    }
    
    // Initialize system calls
    kprintln!("[INIT] Setting up system calls...");
    syscall::init();
    kprintln!("[INIT] System calls initialized");
    
    // Start scheduler
    kprintln!("[INIT] Starting scheduler...");
    kprintln!("");
    kprintln!("CottonOS kernel initialization complete!");
    kprintln!("");
    
    // Check if GUI is available and start it, otherwise use shell
    if drivers::graphics::is_available() {
        kprintln!("Starting GUI desktop...");
        gui::run();
    }
    
    // Fall back to scheduler (which runs shell)
    proc::scheduler::start()
}

/// Panic handler
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // Disable interrupts
    arch::disable_interrupts();
    
    kprintln!("");
    kprintln!("+==========================================================+");
    kprintln!("|                    KERNEL PANIC                          |");
    kprintln!("+==========================================================+");
    
    if let Some(location) = info.location() {
        kprintln!("Location: {}:{}:{}", 
                  location.file(), 
                  location.line(), 
                  location.column());
    }
    
    kprintln!("Message: {}", info.message());
    
    kprintln!("");
    kprintln!("System halted.");
    
    // Halt the CPU
    loop {
        arch::halt();
    }
}

/// Allocation error handler
#[alloc_error_handler]
fn alloc_error_handler(layout: alloc::alloc::Layout) -> ! {
    panic!("Allocation error: {:?}", layout);
}

/// Kernel print macros
#[macro_export]
macro_rules! kprint {
    ($($arg:tt)*) => ({
        use core::fmt::Write;
        let _ = write!($crate::drivers::console::CONSOLE.lock(), $($arg)*);
    });
}

#[macro_export]
macro_rules! kprintln {
    () => ($crate::kprint!("\n"));
    ($($arg:tt)*) => ($crate::kprint!("{}\n", format_args!($($arg)*)));
}

/// Debug print macros (only enabled in debug builds)
#[macro_export]
macro_rules! kdebug {
    ($($arg:tt)*) => ({
        #[cfg(debug_assertions)]
        {
            $crate::kprint!("[DEBUG] ");
            $crate::kprintln!($($arg)*);
        }
    });
}
