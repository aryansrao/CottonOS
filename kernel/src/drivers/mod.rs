//! Device Drivers Module

pub mod console;
pub mod keyboard;
pub mod storage;
pub mod graphics;
pub mod mouse;

/// Initialize all drivers
pub fn init() {
    // Initialize storage FIRST - filesystem needs this
    crate::kprintln!("[DRIVERS] Initializing storage...");
    storage::init();
    
    // Initialize other basic drivers
    console::init();
    keyboard::init();
    
    // Graphics and mouse are initialized later when we have framebuffer info
    crate::kprintln!("[DRIVERS] Device drivers initialized");
}

/// Initialize graphics subsystem with framebuffer info
pub fn init_graphics(addr: u64, width: u32, height: u32, pitch: u32, bpp: u8) {
    graphics::init(addr, width, height, pitch, bpp);
    mouse::init();
}