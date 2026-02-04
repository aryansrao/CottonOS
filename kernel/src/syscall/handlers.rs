//! System Call Handlers

use super::*;
use crate::proc;
use crate::fs;
use alloc::string::String;

/// Exit current process
pub fn sys_exit(status: i32) -> SyscallResult {
    proc::exit(status);
    0 // Never reached
}

/// Fork current process
pub fn sys_fork() -> SyscallResult {
    match proc::fork() {
        Some(pid) => pid.as_u32() as isize,
        None => ENOMEM,
    }
}

/// Execute a new program
pub fn sys_exec(path_ptr: usize, _argv_ptr: usize) -> SyscallResult {
    let path = match read_string_from_user(path_ptr) {
        Some(s) => s,
        None => return EFAULT,
    };
    
    match proc::exec(&path, &[]) {
        Ok(()) => 0,
        Err(_) => ENOEXEC,
    }
}

/// Wait for child process
pub fn sys_wait(pid: usize) -> SyscallResult {
    let pid = proc::ProcessId(pid as u32);
    match proc::wait(pid) {
        Some(status) => status as isize,
        None => ECHILD,
    }
}

/// Get current process ID
pub fn sys_getpid() -> SyscallResult {
    match proc::current() {
        Some(p) => p.pid.as_u32() as isize,
        None => ESRCH,
    }
}

/// Get parent process ID
pub fn sys_getppid() -> SyscallResult {
    match proc::current() {
        Some(p) => {
            match p.parent {
                Some(ppid) => ppid.as_u32() as isize,
                None => 0,
            }
        }
        None => ESRCH,
    }
}

/// Yield CPU
pub fn sys_yield() -> SyscallResult {
    proc::scheduler::yield_now();
    0
}

/// Sleep for milliseconds
pub fn sys_sleep(ms: u64) -> SyscallResult {
    proc::scheduler::sleep_ms(ms);
    0
}

/// Open file
pub fn sys_open(path_ptr: usize, _flags: u32) -> SyscallResult {
    let path = match read_string_from_user(path_ptr) {
        Some(s) => s,
        None => return EFAULT,
    };
    
    match fs::lookup(&path) {
        Ok(_inode) => {
            // TODO: Allocate file descriptor
            0
        }
        Err(_) => ENOENT,
    }
}

/// Close file
pub fn sys_close(_fd: usize) -> SyscallResult {
    // TODO: Close file descriptor
    0
}

/// Read from file
pub fn sys_read(_fd: usize, _buf_ptr: usize, _count: usize) -> SyscallResult {
    // TODO: Implement file read
    ENOSYS
}

/// Write to file
pub fn sys_write(fd: usize, buf_ptr: usize, count: usize) -> SyscallResult {
    // Special case for stdout/stderr
    if fd == 1 || fd == 2 {
        let buf = match read_bytes_from_user(buf_ptr, count) {
            Some(b) => b,
            None => return EFAULT,
        };
        
        for &b in &buf {
            crate::kprint!("{}", b as char);
        }
        
        return count as isize;
    }
    
    // TODO: Implement file write
    ENOSYS
}

/// Seek in file
pub fn sys_seek(_fd: usize, _offset: i64, _whence: u32) -> SyscallResult {
    ENOSYS
}

/// Get file status by path
pub fn sys_stat(path_ptr: usize, stat_ptr: usize) -> SyscallResult {
    let path = match read_string_from_user(path_ptr) {
        Some(s) => s,
        None => return EFAULT,
    };
    
    match fs::stat(&path) {
        Ok(stat) => {
            // Write stat to user buffer
            if !write_to_user(stat_ptr, &stat) {
                return EFAULT;
            }
            0
        }
        Err(_) => ENOENT,
    }
}

/// Get file status by descriptor
pub fn sys_fstat(_fd: usize, _stat_ptr: usize) -> SyscallResult {
    ENOSYS
}

/// Create directory
pub fn sys_mkdir(path_ptr: usize) -> SyscallResult {
    let path = match read_string_from_user(path_ptr) {
        Some(s) => s,
        None => return EFAULT,
    };
    
    match fs::mkdir(&path) {
        Ok(_) => 0,
        Err(_) => EIO,
    }
}

/// Remove directory
pub fn sys_rmdir(path_ptr: usize) -> SyscallResult {
    let path = match read_string_from_user(path_ptr) {
        Some(s) => s,
        None => return EFAULT,
    };
    
    match fs::remove(&path) {
        Ok(()) => 0,
        Err(_) => EIO,
    }
}

/// Unlink file
pub fn sys_unlink(path_ptr: usize) -> SyscallResult {
    let path = match read_string_from_user(path_ptr) {
        Some(s) => s,
        None => return EFAULT,
    };
    
    match fs::remove(&path) {
        Ok(()) => 0,
        Err(_) => EIO,
    }
}

/// Change current directory
pub fn sys_chdir(path_ptr: usize) -> SyscallResult {
    let path = match read_string_from_user(path_ptr) {
        Some(s) => s,
        None => return EFAULT,
    };
    
    // Verify path exists
    match fs::lookup(&path) {
        Ok(inode) => {
            if inode.file_type() != fs::FileType::Directory {
                return ENOTDIR;
            }
            
            // Update process cwd
            // TODO: Update current process cwd
            0
        }
        Err(_) => ENOENT,
    }
}

/// Get current working directory
pub fn sys_getcwd(buf_ptr: usize, size: usize) -> SyscallResult {
    let cwd = match proc::current() {
        Some(p) => p.cwd.clone(),
        None => return ESRCH,
    };
    
    if cwd.len() + 1 > size {
        return EINVAL;
    }
    
    if !write_string_to_user(buf_ptr, &cwd) {
        return EFAULT;
    }
    
    cwd.len() as isize
}

/// Set program break (memory allocation)
pub fn sys_brk(_addr: usize) -> SyscallResult {
    // TODO: Implement brk
    ENOSYS
}

/// Get system information
pub fn sys_uname(buf_ptr: usize) -> SyscallResult {
    #[repr(C)]
    struct Uname {
        sysname: [u8; 65],
        nodename: [u8; 65],
        release: [u8; 65],
        version: [u8; 65],
        machine: [u8; 65],
    }
    
    let mut uname = Uname {
        sysname: [0; 65],
        nodename: [0; 65],
        release: [0; 65],
        version: [0; 65],
        machine: [0; 65],
    };
    
    copy_str_to_array(&mut uname.sysname, "CottonOS");
    copy_str_to_array(&mut uname.nodename, "cotton");
    copy_str_to_array(&mut uname.release, "0.1.0");
    copy_str_to_array(&mut uname.version, "#1");
    
    #[cfg(target_arch = "x86_64")]
    copy_str_to_array(&mut uname.machine, "x86_64");
    
    #[cfg(target_arch = "aarch64")]
    copy_str_to_array(&mut uname.machine, "aarch64");
    
    if !write_to_user(buf_ptr, &uname) {
        return EFAULT;
    }
    
    0
}

fn copy_str_to_array(arr: &mut [u8], s: &str) {
    let bytes = s.as_bytes();
    let len = bytes.len().min(arr.len() - 1);
    arr[..len].copy_from_slice(&bytes[..len]);
}

/// Get current time
pub fn sys_time() -> SyscallResult {
    // Return ticks as approximation
    proc::scheduler::ticks() as isize
}

/// Get system uptime
pub fn sys_uptime() -> SyscallResult {
    proc::scheduler::ticks() as isize
}

// Helper functions for user memory access

/// Read string from user space
fn read_string_from_user(ptr: usize) -> Option<String> {
    // In a real implementation, this would verify the pointer
    // is in user space and readable
    if ptr == 0 {
        return None;
    }
    
    let mut s = String::new();
    let mut addr = ptr;
    
    loop {
        let byte = unsafe { *(addr as *const u8) };
        if byte == 0 {
            break;
        }
        s.push(byte as char);
        addr += 1;
        
        // Limit string length
        if s.len() > 4096 {
            return None;
        }
    }
    
    Some(s)
}

/// Read bytes from user space
fn read_bytes_from_user(ptr: usize, len: usize) -> Option<alloc::vec::Vec<u8>> {
    if ptr == 0 {
        return None;
    }
    
    let slice = unsafe { core::slice::from_raw_parts(ptr as *const u8, len) };
    Some(slice.to_vec())
}

/// Write to user space
fn write_to_user<T>(ptr: usize, data: &T) -> bool {
    if ptr == 0 {
        return false;
    }
    
    unsafe {
        core::ptr::write(ptr as *mut T, core::ptr::read(data));
    }
    true
}

/// Write string to user space
fn write_string_to_user(ptr: usize, s: &str) -> bool {
    if ptr == 0 {
        return false;
    }
    
    let bytes = s.as_bytes();
    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, bytes.len());
        *((ptr + bytes.len()) as *mut u8) = 0;
    }
    true
}
