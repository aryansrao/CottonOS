//! CottonOS Userspace Library and Shell
//!
//! This crate provides userspace utilities and the shell

#![no_std]
#![no_main]

extern crate alloc;

pub mod shell;
pub mod syscall;

use core::panic::PanicInfo;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
