//! System Call Module
//!
//! System call interface for user programs

pub mod handlers;

use core::arch::asm;

/// System call numbers
pub mod syscall_numbers {
    // Process management
    pub const SYS_EXIT: usize = 0;
    pub const SYS_FORK: usize = 1;
    pub const SYS_EXEC: usize = 2;
    pub const SYS_WAIT: usize = 3;
    pub const SYS_GETPID: usize = 4;
    pub const SYS_GETPPID: usize = 5;
    pub const SYS_YIELD: usize = 6;
    pub const SYS_SLEEP: usize = 7;
    
    // File operations
    pub const SYS_OPEN: usize = 10;
    pub const SYS_CLOSE: usize = 11;
    pub const SYS_READ: usize = 12;
    pub const SYS_WRITE: usize = 13;
    pub const SYS_SEEK: usize = 14;
    pub const SYS_STAT: usize = 15;
    pub const SYS_FSTAT: usize = 16;
    
    // Directory operations
    pub const SYS_MKDIR: usize = 20;
    pub const SYS_RMDIR: usize = 21;
    pub const SYS_UNLINK: usize = 22;
    pub const SYS_READDIR: usize = 23;
    pub const SYS_CHDIR: usize = 24;
    pub const SYS_GETCWD: usize = 25;
    
    // Memory management
    pub const SYS_BRK: usize = 30;
    pub const SYS_MMAP: usize = 31;
    pub const SYS_MUNMAP: usize = 32;
    
    // System info
    pub const SYS_UNAME: usize = 40;
    pub const SYS_TIME: usize = 41;
    pub const SYS_UPTIME: usize = 42;
    
    // I/O
    pub const SYS_IOCTL: usize = 50;
    pub const SYS_DUP: usize = 51;
    pub const SYS_DUP2: usize = 52;
    pub const SYS_PIPE: usize = 53;
}

pub use syscall_numbers::*;

/// System call result
pub type SyscallResult = isize;

/// System call error codes
pub mod errno {
    pub const EPERM: isize = -1;
    pub const ENOENT: isize = -2;
    pub const ESRCH: isize = -3;
    pub const EINTR: isize = -4;
    pub const EIO: isize = -5;
    pub const ENXIO: isize = -6;
    pub const E2BIG: isize = -7;
    pub const ENOEXEC: isize = -8;
    pub const EBADF: isize = -9;
    pub const ECHILD: isize = -10;
    pub const EAGAIN: isize = -11;
    pub const ENOMEM: isize = -12;
    pub const EACCES: isize = -13;
    pub const EFAULT: isize = -14;
    pub const EBUSY: isize = -16;
    pub const EEXIST: isize = -17;
    pub const ENODEV: isize = -19;
    pub const ENOTDIR: isize = -20;
    pub const EISDIR: isize = -21;
    pub const EINVAL: isize = -22;
    pub const ENFILE: isize = -23;
    pub const EMFILE: isize = -24;
    pub const ENOTTY: isize = -25;
    pub const EFBIG: isize = -27;
    pub const ENOSPC: isize = -28;
    pub const ESPIPE: isize = -29;
    pub const EROFS: isize = -30;
    pub const EPIPE: isize = -32;
    pub const ENOSYS: isize = -38;
    pub const ENOTEMPTY: isize = -39;
}

pub use errno::*;

/// Initialize system call interface
pub fn init() {
    #[cfg(target_arch = "x86_64")]
    init_x86_64();
    
    #[cfg(target_arch = "aarch64")]
    init_aarch64();
    
    crate::kprintln!("[SYSCALL] System call interface initialized");
}

/// Initialize x86_64 system call interface (via interrupt 0x80 or syscall)
#[cfg(target_arch = "x86_64")]
fn init_x86_64() {
    use crate::arch::x86_64::wrmsr;
    
    // Set up SYSCALL/SYSRET MSRs
    const MSR_STAR: u32 = 0xC0000081;
    const MSR_LSTAR: u32 = 0xC0000082;
    const MSR_FMASK: u32 = 0xC0000084;
    
    // STAR: bits 32-47 = kernel CS, bits 48-63 = user CS
    let star = (0x08u64 << 32) | (0x1Bu64 << 48);
    wrmsr(MSR_STAR, star);
    
    // LSTAR: syscall entry point
    wrmsr(MSR_LSTAR, syscall_entry_x86_64 as u64);
    
    // FMASK: flags to clear on syscall
    wrmsr(MSR_FMASK, 0x200); // Clear IF
}

#[cfg(target_arch = "x86_64")]
extern "C" fn syscall_entry_x86_64() {
    // This is called via SYSCALL instruction
    // RAX = syscall number
    // RDI, RSI, RDX, R10, R8, R9 = arguments
    // Returns result in RAX
}

#[cfg(target_arch = "aarch64")]
fn init_aarch64() {
    // ARM uses SVC instruction for system calls
    // The exception handler routes to our syscall handler
}

/// Handle system call (called from interrupt/exception handler)
pub fn handle(num: usize, arg1: usize, arg2: usize, arg3: usize, arg4: usize, arg5: usize) -> SyscallResult {
    match num {
        // Process management
        SYS_EXIT => handlers::sys_exit(arg1 as i32),
        SYS_FORK => handlers::sys_fork(),
        SYS_EXEC => handlers::sys_exec(arg1, arg2),
        SYS_WAIT => handlers::sys_wait(arg1),
        SYS_GETPID => handlers::sys_getpid(),
        SYS_GETPPID => handlers::sys_getppid(),
        SYS_YIELD => handlers::sys_yield(),
        SYS_SLEEP => handlers::sys_sleep(arg1 as u64),
        
        // File operations
        SYS_OPEN => handlers::sys_open(arg1, arg2 as u32),
        SYS_CLOSE => handlers::sys_close(arg1),
        SYS_READ => handlers::sys_read(arg1, arg2, arg3),
        SYS_WRITE => handlers::sys_write(arg1, arg2, arg3),
        SYS_SEEK => handlers::sys_seek(arg1, arg2 as i64, arg3 as u32),
        SYS_STAT => handlers::sys_stat(arg1, arg2),
        SYS_FSTAT => handlers::sys_fstat(arg1, arg2),
        
        // Directory operations
        SYS_MKDIR => handlers::sys_mkdir(arg1),
        SYS_RMDIR => handlers::sys_rmdir(arg1),
        SYS_UNLINK => handlers::sys_unlink(arg1),
        SYS_CHDIR => handlers::sys_chdir(arg1),
        SYS_GETCWD => handlers::sys_getcwd(arg1, arg2),
        
        // Memory management
        SYS_BRK => handlers::sys_brk(arg1),
        
        // System info
        SYS_UNAME => handlers::sys_uname(arg1),
        SYS_TIME => handlers::sys_time(),
        SYS_UPTIME => handlers::sys_uptime(),
        
        _ => ENOSYS,
    }
}

/// User-space system call interface (for kernel internal use or testing)
#[cfg(target_arch = "x86_64")]
#[inline(always)]
pub unsafe fn syscall0(num: usize) -> isize {
    let ret: isize;
    asm!(
        "syscall",
        inlateout("rax") num => ret,
        out("rcx") _,
        out("r11") _,
        options(nostack, preserves_flags)
    );
    ret
}

#[cfg(target_arch = "x86_64")]
#[inline(always)]
pub unsafe fn syscall1(num: usize, arg1: usize) -> isize {
    let ret: isize;
    asm!(
        "syscall",
        inlateout("rax") num => ret,
        in("rdi") arg1,
        out("rcx") _,
        out("r11") _,
        options(nostack, preserves_flags)
    );
    ret
}

#[cfg(target_arch = "x86_64")]
#[inline(always)]
pub unsafe fn syscall3(num: usize, arg1: usize, arg2: usize, arg3: usize) -> isize {
    let ret: isize;
    asm!(
        "syscall",
        inlateout("rax") num => ret,
        in("rdi") arg1,
        in("rsi") arg2,
        in("rdx") arg3,
        out("rcx") _,
        out("r11") _,
        options(nostack, preserves_flags)
    );
    ret
}

#[cfg(target_arch = "aarch64")]
#[inline(always)]
pub unsafe fn syscall0(num: usize) -> isize {
    let ret: isize;
    asm!(
        "svc #0",
        inlateout("x8") num => _,
        lateout("x0") ret,
        options(nostack)
    );
    ret
}

#[cfg(target_arch = "aarch64")]
#[inline(always)]
pub unsafe fn syscall1(num: usize, arg1: usize) -> isize {
    let ret: isize;
    asm!(
        "svc #0",
        inlateout("x8") num => _,
        inlateout("x0") arg1 => ret,
        options(nostack)
    );
    ret
}

#[cfg(target_arch = "aarch64")]
#[inline(always)]
pub unsafe fn syscall3(num: usize, arg1: usize, arg2: usize, arg3: usize) -> isize {
    let ret: isize;
    asm!(
        "svc #0",
        inlateout("x8") num => _,
        inlateout("x0") arg1 => ret,
        in("x1") arg2,
        in("x2") arg3,
        options(nostack)
    );
    ret
}
