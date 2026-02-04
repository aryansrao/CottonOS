//! Userspace System Call Interface

/// System call numbers (must match kernel)
pub const SYS_EXIT: usize = 0;
pub const SYS_FORK: usize = 1;
pub const SYS_EXEC: usize = 2;
pub const SYS_WAIT: usize = 3;
pub const SYS_GETPID: usize = 4;
pub const SYS_GETPPID: usize = 5;
pub const SYS_YIELD: usize = 6;
pub const SYS_SLEEP: usize = 7;

pub const SYS_OPEN: usize = 10;
pub const SYS_CLOSE: usize = 11;
pub const SYS_READ: usize = 12;
pub const SYS_WRITE: usize = 13;
pub const SYS_STAT: usize = 15;

pub const SYS_MKDIR: usize = 20;
pub const SYS_RMDIR: usize = 21;
pub const SYS_UNLINK: usize = 22;
pub const SYS_CHDIR: usize = 24;
pub const SYS_GETCWD: usize = 25;

pub const SYS_UNAME: usize = 40;
pub const SYS_TIME: usize = 41;

#[cfg(target_arch = "x86_64")]
mod arch {
    use core::arch::asm;
    
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
    
    #[inline(always)]
    pub unsafe fn syscall2(num: usize, arg1: usize, arg2: usize) -> isize {
        let ret: isize;
        asm!(
            "syscall",
            inlateout("rax") num => ret,
            in("rdi") arg1,
            in("rsi") arg2,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
        ret
    }
    
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
}

#[cfg(target_arch = "aarch64")]
mod arch {
    use core::arch::asm;
    
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
    
    #[inline(always)]
    pub unsafe fn syscall2(num: usize, arg1: usize, arg2: usize) -> isize {
        let ret: isize;
        asm!(
            "svc #0",
            inlateout("x8") num => _,
            inlateout("x0") arg1 => ret,
            in("x1") arg2,
            options(nostack)
        );
        ret
    }
    
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
}

pub use arch::*;

// High-level syscall wrappers

pub fn exit(status: i32) -> ! {
    unsafe { syscall1(SYS_EXIT, status as usize) };
    loop {}
}

pub fn fork() -> isize {
    unsafe { syscall0(SYS_FORK) }
}

pub fn getpid() -> u32 {
    unsafe { syscall0(SYS_GETPID) as u32 }
}

pub fn getppid() -> u32 {
    unsafe { syscall0(SYS_GETPPID) as u32 }
}

pub fn yield_now() {
    unsafe { syscall0(SYS_YIELD) };
}

pub fn sleep(ms: u64) {
    unsafe { syscall1(SYS_SLEEP, ms as usize) };
}

pub fn write(fd: usize, buf: &[u8]) -> isize {
    unsafe { syscall3(SYS_WRITE, fd, buf.as_ptr() as usize, buf.len()) }
}

pub fn read(fd: usize, buf: &mut [u8]) -> isize {
    unsafe { syscall3(SYS_READ, fd, buf.as_ptr() as usize, buf.len()) }
}

pub fn open(path: &str, flags: u32) -> isize {
    unsafe { syscall2(SYS_OPEN, path.as_ptr() as usize, flags as usize) }
}

pub fn close(fd: usize) -> isize {
    unsafe { syscall1(SYS_CLOSE, fd) }
}

pub fn mkdir(path: &str) -> isize {
    unsafe { syscall1(SYS_MKDIR, path.as_ptr() as usize) }
}

pub fn chdir(path: &str) -> isize {
    unsafe { syscall1(SYS_CHDIR, path.as_ptr() as usize) }
}

pub fn getcwd(buf: &mut [u8]) -> isize {
    unsafe { syscall2(SYS_GETCWD, buf.as_ptr() as usize, buf.len()) }
}

/// Print to stdout
pub fn print(s: &str) {
    write(1, s.as_bytes());
}

/// Print line to stdout
pub fn println(s: &str) {
    print(s);
    print("\n");
}
