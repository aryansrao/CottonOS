//! CottonOS Kernel Shell
//!
//! Simple interactive shell for testing and debugging

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use crate::kprint;
use crate::kprintln;

/// Current working directory
static mut CWD: Option<String> = None;

/// Whether disk is available
static mut HAS_DISK: bool = false;

/// Get current working directory
pub fn get_cwd() -> String {
    unsafe {
        CWD.clone().unwrap_or_else(|| String::from("/"))
    }
}

fn set_cwd(path: String) {
    unsafe {
        CWD = Some(path);
    }
}

/// Check if disk is available
fn has_disk() -> bool {
    unsafe { HAS_DISK }
}

/// Set disk availability
fn set_has_disk(val: bool) {
    unsafe { HAS_DISK = val; }
}

/// Resolve a path (handle relative paths)
pub fn resolve_path(path: &str) -> String {
    if path.starts_with('/') {
        String::from(path)
    } else {
        let cwd = get_cwd();
        if cwd == "/" {
            format!("/{}", path)
        } else {
            format!("{}/{}", cwd, path)
        }
    }
}

/// Execute a shell command and return output as String (for GUI terminal)
pub fn execute_command(line: &str) -> String {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.is_empty() {
        return String::new();
    }
    
    let cmd = parts[0];
    let args = &parts[1..];
    
    match cmd {
        "help" => {
            if args.is_empty() {
                String::from("Commands: help, clear, info, mem, df, ps, uptime, echo, sync, reboot, halt\nFiles:    ls, cd, pwd, cat, touch, mkdir, rm, write\n\nFiles are stored persistently on disk (CottonFS).")
            } else {
                exec_help_detail(args[0])
            }
        }
        "clear" => String::from("\x1b[CLEAR]"),
        "info" => exec_info(),
        "mem" => exec_mem(),
        "df" => exec_df(),
        "sync" => exec_sync(),
        "ps" => exec_ps(),
        "uptime" => exec_uptime(),
        "echo" => args.join(" "),
        "panic" => { panic!("User-triggered panic"); }
        "reboot" => { cmd_reboot(); String::from("Rebooting...") }
        "halt" => { cmd_halt(); String::from("System halted.") }
        "ls" => exec_ls(args),
        "cd" => exec_cd(args),
        "pwd" => get_cwd(),
        "cat" => exec_cat(args),
        "touch" => exec_touch(args),
        "mkdir" => exec_mkdir(args),
        "rm" => exec_rm(args),
        "write" => exec_write(args),
        _ => format!("Unknown command: '{}'. Type 'help'.", cmd),
    }
}

fn exec_help_detail(cmd: &str) -> String {
    match cmd {
        "ls" => String::from("ls [path] - List directory contents"),
        "cd" => String::from("cd <path> - Change directory"),
        "pwd" => String::from("pwd - Print working directory"),
        "cat" => String::from("cat <file> - Display file contents"),
        "touch" => String::from("touch <file> - Create empty file"),
        "mkdir" => String::from("mkdir <dir> - Create directory"),
        "rm" => String::from("rm <file> - Remove file or empty directory"),
        "write" => String::from("write <file> <text> - Write text to file"),
        "df" => String::from("df - Show disk space usage (CottonFS)"),
        "sync" => String::from("sync - Force sync all data to disk"),
        "info" => String::from("info - Show system information"),
        "mem" => String::from("mem - Show memory statistics"),
        "ps" => String::from("ps - List running processes"),
        "uptime" => String::from("uptime - Show system uptime"),
        "echo" => String::from("echo <text> - Print text"),
        "clear" => String::from("clear - Clear the screen"),
        "reboot" => String::from("reboot - Restart the system"),
        "halt" => String::from("halt - Stop the CPU"),
        _ => format!("Unknown command: {}", cmd),
    }
}

fn exec_info() -> String {
    format!("+--------------------------------------------+\n|           CottonOS System Info             |\n+--------------------------------------------+\n|  Kernel Version: {}                     |\n|  Architecture:   {:?}                  |\n|  Filesystem:     CottonFS (persistent)    |\n+--------------------------------------------+",
        crate::KERNEL_VERSION, crate::Architecture::current())
}

fn exec_mem() -> String {
    let (total, used, free) = crate::mm::physical::stats();
    format!("Memory Statistics:\n  Total:     {} KB ({} MB)\n  Used:      {} KB ({} MB)\n  Free:      {} KB ({} MB)\n  Usage:     {}%",
        total / 1024, total / (1024 * 1024),
        used / 1024, used / (1024 * 1024),
        free / 1024, free / (1024 * 1024),
        if total > 0 { (used * 100) / total } else { 0 })
}

fn exec_df() -> String {
    if let Some(info) = crate::fs::get_storage_info() {
        format!("Filesystem: CottonFS\n\
                 Storage Statistics:\n\
                 +-----------------+-----------+\n\
                 | Total           | {:>9} |\n\
                 | Used            | {:>9} |\n\
                 | Free            | {:>9} |\n\
                 | Usage           | {:>8}% |\n\
                 +-----------------+-----------+\n\
                 | Files (inodes)  | {:>4}/{:<4} |\n\
                 +-----------------+-----------+",
            info.total_display(),
            info.used_display(),
            info.free_display(),
            info.usage_percent(),
            info.used_inodes,
            info.total_inodes)
    } else {
        String::from("Filesystem: RAM only (no persistent storage)\nNo disk statistics available.")
    }
}

fn exec_sync() -> String {
    crate::fs::sync_all();
    String::from("Filesystem synced to disk.")
}

fn exec_ps() -> String {
    let (queued, running, _ticks) = crate::proc::scheduler::stats();
    format!("Process List:\n  PID  STATE      NAME\n  ---  -----      ----\n  0    Running    kernel\n\nTotal: {} queued, {} running", queued, running)
}

fn exec_uptime() -> String {
    let ticks = crate::proc::scheduler::ticks();
    let seconds = ticks / 1000;
    let minutes = seconds / 60;
    let hours = minutes / 60;
    format!("Uptime: {}h {}m {}s ({} ticks)", hours, minutes % 60, seconds % 60, ticks)
}

fn exec_ls(args: &[&str]) -> String {
    let path = if args.is_empty() {
        get_cwd()
    } else {
        resolve_path(args[0])
    };
    
    match crate::fs::readdir(&path) {
        Ok(entries) => {
            if entries.is_empty() {
                String::from("(empty directory)")
            } else {
                let mut result = String::new();
                for entry in entries {
                    let type_char = match entry.file_type {
                        crate::fs::FileType::Directory => 'd',
                        crate::fs::FileType::Regular => '-',
                        crate::fs::FileType::Symlink => 'l',
                        crate::fs::FileType::CharDevice => 'c',
                        crate::fs::FileType::BlockDevice => 'b',
                        _ => '?',
                    };
                    
                    let full_path = if path == "/" {
                        format!("/{}", entry.name)
                    } else {
                        format!("{}/{}", path, entry.name)
                    };
                    
                    let size = match crate::fs::stat(&full_path) {
                        Ok(stat) => stat.size,
                        Err(_) => 0,
                    };
                    
                    result.push_str(&format!("{} {:>8} {}\n", type_char, size, entry.name));
                }
                result
            }
        }
        Err(e) => format!("ls: {}: {}", path, e),
    }
}

fn exec_cd(args: &[&str]) -> String {
    if args.is_empty() {
        set_cwd(String::from("/"));
        return String::new();
    }
    
    let path = resolve_path(args[0]);
    
    match crate::fs::lookup(&path) {
        Ok(inode) => {
            if inode.file_type() == crate::fs::FileType::Directory {
                let normalized = normalize_path(&path);
                set_cwd(normalized);
                String::new()
            } else {
                format!("cd: {}: Not a directory", args[0])
            }
        }
        Err(e) => format!("cd: {}: {}", args[0], e),
    }
}

fn exec_cat(args: &[&str]) -> String {
    if args.is_empty() {
        return String::from("cat: missing file argument");
    }
    
    let path = resolve_path(args[0]);
    
    match crate::fs::lookup(&path) {
        Ok(inode) => {
            if inode.file_type() != crate::fs::FileType::Regular {
                return format!("cat: {}: Not a regular file", args[0]);
            }
            
            let mut result = String::new();
            let mut buf = [0u8; 256];
            let mut offset = 0u64;
            
            loop {
                match inode.read(offset, &mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        for &byte in &buf[..n] {
                            if byte >= 0x20 && byte <= 0x7E || byte == b'\n' || byte == b'\r' || byte == b'\t' {
                                result.push(byte as char);
                            }
                        }
                        offset += n as u64;
                    }
                    Err(e) => {
                        result.push_str(&format!("\ncat: read error: {}", e));
                        break;
                    }
                }
            }
            result
        }
        Err(e) => format!("cat: {}: {}", args[0], e),
    }
}

fn exec_touch(args: &[&str]) -> String {
    if args.is_empty() {
        return String::from("touch: missing file argument");
    }
    
    let path = resolve_path(args[0]);
    
    if crate::fs::lookup(&path).is_ok() {
        return String::new(); // File exists, touch does nothing
    }
    
    match crate::fs::create(&path) {
        Ok(_) => format!("Created: {}", path),
        Err(e) => format!("touch: {}: {}", args[0], e),
    }
}

fn exec_mkdir(args: &[&str]) -> String {
    if args.is_empty() {
        return String::from("mkdir: missing directory argument");
    }
    
    let path = resolve_path(args[0]);
    
    match crate::fs::mkdir(&path) {
        Ok(_) => format!("Created directory: {}", path),
        Err(e) => format!("mkdir: {}: {}", args[0], e),
    }
}

fn exec_rm(args: &[&str]) -> String {
    if args.is_empty() {
        return String::from("rm: missing file argument");
    }
    
    let path = resolve_path(args[0]);
    
    match crate::fs::remove(&path) {
        Ok(_) => format!("Removed: {}", path),
        Err(e) => format!("rm: {}: {}", args[0], e),
    }
}

fn exec_write(args: &[&str]) -> String {
    if args.len() < 2 {
        return String::from("write: usage: write <file> <text>");
    }
    
    let path = resolve_path(args[0]);
    let text = args[1..].join(" ");
    
    let inode = match crate::fs::lookup(&path) {
        Ok(i) => i,
        Err(_) => {
            match crate::fs::create(&path) {
                Ok(i) => i,
                Err(e) => return format!("write: cannot create {}: {}", args[0], e),
            }
        }
    };
    
    match inode.write(0, text.as_bytes()) {
        Ok(n) => format!("Wrote {} bytes to {}", n, path),
        Err(e) => format!("write: {}: {}", args[0], e),
    }
}

/// Run the kernel shell
pub fn run() -> ! {
    set_cwd(String::from("/"));
    
    // Check for disk and auto-load on startup
    init_disk();
    
    kprintln!("");
    kprintln!("+-------------------------------------------+");
    kprintln!("|     Welcome to CottonOS Shell v0.1.0      |");
    kprintln!("|       Type 'help' for commands            |");
    kprintln!("+-------------------------------------------+");
    kprintln!("");
    
    let mut input = String::new();
    
    loop {
        kprint!("cotton:{}> ", get_cwd());
        
        // Read input
        input.clear();
        read_line(&mut input);
        
        let line = input.trim();
        if line.is_empty() {
            continue;
        }
        
        // Parse command
        let parts: Vec<&str> = line.split_whitespace().collect();
        let cmd = parts[0];
        let args = &parts[1..];
        
        // Execute command
        match cmd {
            "help" => {
                if args.is_empty() {
                    cmd_help();
                } else {
                    cmd_help_detail(args[0]);
                }
            }
            "clear" => cmd_clear(),
            "info" => cmd_info(),
            "mem" => cmd_mem(),
            "df" => cmd_df(),
            "sync" => cmd_sync(),
            "ps" => cmd_ps(),
            "uptime" => cmd_uptime(),
            "echo" => cmd_echo(args),
            "panic" => cmd_panic(),
            "reboot" => cmd_reboot(),
            "halt" => cmd_halt(),
            // File commands
            "ls" => cmd_ls(args),
            "cd" => cmd_cd(args),
            "pwd" => cmd_pwd(),
            "cat" => cmd_cat(args),
            "touch" => cmd_touch(args),
            "mkdir" => cmd_mkdir(args),
            "rm" => cmd_rm(args),
            "write" => cmd_write(args),
            _ => kprintln!("Unknown command: '{}'. Type 'help'.", cmd),
        }
    }
}

/// Read a line from keyboard input
fn read_line(buf: &mut String) {
    loop {
        // Wait for key
        while !crate::drivers::keyboard::has_key() {
            crate::arch::halt();
        }
        
        // Use get_char which skips non-printable events like key releases
        if let Some(c) = crate::drivers::keyboard::get_char() {
            match c {
                '\n' | '\r' => {
                    kprintln!("");
                    return;
                }
                '\x08' | '\x7F' => {
                    // Backspace - remove last char and update display
                    if !buf.is_empty() {
                        buf.pop();
                        // Move cursor back, print space, move cursor back
                        kprint!("{}", '\x08');
                        kprint!(" ");
                        kprint!("{}", '\x08');
                    }
                }
                c if c >= ' ' && c <= '~' => {
                    // Only accept printable ASCII
                    buf.push(c);
                    kprint!("{}", c);
                }
                _ => {}
            }
        }
    }
}

fn cmd_help() {
    kprintln!("Commands: help, clear, info, mem, df, ps, uptime, echo, sync, reboot, halt");
    kprintln!("Files:    ls, cd, pwd, cat, touch, mkdir, rm, write");
    kprintln!("");
    kprintln!("Files are stored persistently on disk (CottonFS).");
}

fn cmd_help_detail(cmd: &str) {
    match cmd {
        "ls" => kprintln!("ls [path] - List directory contents"),
        "cd" => kprintln!("cd <path> - Change directory"),
        "pwd" => kprintln!("pwd - Print working directory"),
        "cat" => kprintln!("cat <file> - Display file contents"),
        "touch" => kprintln!("touch <file> - Create empty file"),
        "mkdir" => kprintln!("mkdir <dir> - Create directory"),
        "rm" => kprintln!("rm <file> - Remove file or empty directory"),
        "write" => kprintln!("write <file> <text> - Write text to file"),
        "df" => kprintln!("df - Show disk space usage (CottonFS)"),
        "sync" => kprintln!("sync - Force write all files to disk"),
        "info" => kprintln!("info - Show system information"),
        "mem" => kprintln!("mem - Show memory statistics"),
        "ps" => kprintln!("ps - List running processes"),
        "uptime" => kprintln!("uptime - Show system uptime"),
        "echo" => kprintln!("echo <text> - Print text"),
        "clear" => kprintln!("clear - Clear the screen"),
        "reboot" => kprintln!("reboot - Restart the system"),
        "halt" => kprintln!("halt - Stop the CPU"),
        "panic" => kprintln!("panic - Trigger kernel panic (testing)"),
        _ => kprintln!("Unknown command: {}", cmd),
    }
}

fn cmd_clear() {
    // Clear screen by printing newlines or using VGA clear
    #[cfg(target_arch = "x86_64")]
    {
        let mut console = crate::drivers::console::CONSOLE.lock();
        console.clear();
    }
}

fn cmd_info() {
    kprintln!("+--------------------------------------------+");
    kprintln!("|           CottonOS System Info             |");
    kprintln!("+--------------------------------------------+");
    kprintln!("|  Kernel Version: {}                     |", crate::KERNEL_VERSION);
    kprintln!("|  Architecture:   {:?}                  |", crate::Architecture::current());
    kprintln!("|  Filesystem:     CottonFS (persistent)    |");
    kprintln!("+--------------------------------------------+");
}

fn cmd_mem() {
    let (total, used, free) = crate::mm::physical::stats();
    kprintln!("Memory Statistics:");
    kprintln!("  Total:     {} KB ({} MB)", total / 1024, total / (1024 * 1024));
    kprintln!("  Used:      {} KB ({} MB)", used / 1024, used / (1024 * 1024));
    kprintln!("  Free:      {} KB ({} MB)", free / 1024, free / (1024 * 1024));
    kprintln!("  Usage:     {}%", if total > 0 { (used * 100) / total } else { 0 });
}

fn cmd_df() {
    kprintln!("Disk Space Usage (CottonFS):");
    if let Some(info) = crate::fs::get_storage_info() {
        kprintln!("+-----------------+-----------+");
        kprintln!("| Total           | {:>9} |", info.total_display());
        kprintln!("| Used            | {:>9} |", info.used_display());
        kprintln!("| Free            | {:>9} |", info.free_display());
        kprintln!("| Usage           | {:>8}% |", info.usage_percent());
        kprintln!("+-----------------+-----------+");
        kprintln!("| Files (inodes)  | {:>4}/{:<4} |", info.used_inodes, info.total_inodes);
        kprintln!("+-----------------+-----------+");
    } else {
        kprintln!("  RAM-only filesystem (no persistent storage)");
    }
}

fn cmd_sync() {
    crate::fs::sync_all();
}

fn cmd_ps() {
    kprintln!("Process List:");
    kprintln!("  PID  STATE      NAME");
    kprintln!("  ---  -----      ----");
    
    // Get process info
    let (queued, running, _ticks) = crate::proc::scheduler::stats();
    kprintln!("  0    Running    kernel");
    kprintln!("");
    kprintln!("Total: {} queued, {} running", queued, running);
}

fn cmd_uptime() {
    let ticks = crate::proc::scheduler::ticks();
    let seconds = ticks / 1000;
    let minutes = seconds / 60;
    let hours = minutes / 60;
    
    kprintln!("Uptime: {}h {}m {}s ({} ticks)", 
              hours, minutes % 60, seconds % 60, ticks);
}

fn cmd_echo(args: &[&str]) {
    kprintln!("{}", args.join(" "));
}

fn cmd_panic() {
    panic!("User-triggered panic via shell command");
}

fn cmd_reboot() {
    kprintln!("Rebooting...");
    #[cfg(target_arch = "x86_64")]
    unsafe {
        // Try keyboard controller reset
        let mut good = false;
        for _ in 0..1000 {
            if crate::arch::x86_64::inb(0x64) & 0x02 == 0 {
                good = true;
                break;
            }
        }
        if good {
            crate::arch::x86_64::outb(0x64, 0xFE);
        }
        
        // If that fails, triple fault
        crate::arch::disable_interrupts();
        core::arch::asm!("lidt [{}]", in(reg) &[0u64; 2], options(nostack));
        core::arch::asm!("int3", options(nostack));
    }
    loop { crate::arch::halt(); }
}

fn cmd_halt() {
    kprintln!("System halted.");
    crate::arch::disable_interrupts();
    loop {
        crate::arch::halt();
    }
}
// ==================== FILE COMMANDS ====================

fn cmd_ls(args: &[&str]) {
    let path = if args.is_empty() {
        get_cwd()
    } else {
        resolve_path(args[0])
    };
    
    match crate::fs::readdir(&path) {
        Ok(entries) => {
            if entries.is_empty() {
                kprintln!("(empty directory)");
            } else {
                for entry in entries {
                    let type_char = match entry.file_type {
                        crate::fs::FileType::Directory => 'd',
                        crate::fs::FileType::Regular => '-',
                        crate::fs::FileType::Symlink => 'l',
                        crate::fs::FileType::CharDevice => 'c',
                        crate::fs::FileType::BlockDevice => 'b',
                        _ => '?',
                    };
                    
                    // Try to get file size
                    let full_path = if path == "/" {
                        format!("/{}", entry.name)
                    } else {
                        format!("{}/{}", path, entry.name)
                    };
                    
                    let size = match crate::fs::stat(&full_path) {
                        Ok(stat) => stat.size,
                        Err(_) => 0,
                    };
                    
                    kprintln!("{} {:>8} {}", type_char, size, entry.name);
                }
            }
        }
        Err(e) => kprintln!("ls: {}: {}", path, e),
    }
}

fn cmd_cd(args: &[&str]) {
    if args.is_empty() {
        set_cwd(String::from("/"));
        return;
    }
    
    let path = resolve_path(args[0]);
    
    // Verify it's a directory
    match crate::fs::lookup(&path) {
        Ok(inode) => {
            if inode.file_type() == crate::fs::FileType::Directory {
                // Normalize the path
                let normalized = normalize_path(&path);
                set_cwd(normalized);
            } else {
                kprintln!("cd: {}: Not a directory", args[0]);
            }
        }
        Err(e) => kprintln!("cd: {}: {}", args[0], e),
    }
}

fn normalize_path(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => { parts.pop(); }
            p => parts.push(p),
        }
    }
    
    if parts.is_empty() {
        String::from("/")
    } else {
        format!("/{}", parts.join("/"))
    }
}

fn cmd_pwd() {
    kprintln!("{}", get_cwd());
}

fn cmd_cat(args: &[&str]) {
    if args.is_empty() {
        kprintln!("cat: missing file argument");
        return;
    }
    
    let path = resolve_path(args[0]);
    
    match crate::fs::lookup(&path) {
        Ok(inode) => {
            if inode.file_type() != crate::fs::FileType::Regular {
                kprintln!("cat: {}: Not a regular file", args[0]);
                return;
            }
            
            let mut buf = [0u8; 256];
            let mut offset = 0u64;
            
            loop {
                match inode.read(offset, &mut buf) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        for &byte in &buf[..n] {
                            if byte >= 0x20 && byte <= 0x7E || byte == b'\n' || byte == b'\r' || byte == b'\t' {
                                kprint!("{}", byte as char);
                            }
                        }
                        offset += n as u64;
                    }
                    Err(e) => {
                        kprintln!("cat: read error: {}", e);
                        break;
                    }
                }
            }
            kprintln!(""); // Ensure newline at end
        }
        Err(e) => kprintln!("cat: {}: {}", args[0], e),
    }
}

fn cmd_touch(args: &[&str]) {
    if args.is_empty() {
        kprintln!("touch: missing file argument");
        return;
    }
    
    let path = resolve_path(args[0]);
    
    // Check if file already exists
    if crate::fs::lookup(&path).is_ok() {
        // File exists, do nothing (touch behavior)
        return;
    }
    
    match crate::fs::create(&path) {
        Ok(_) => kprintln!("Created: {}", path),
        Err(e) => kprintln!("touch: {}: {}", args[0], e),
    }
}

fn cmd_mkdir(args: &[&str]) {
    if args.is_empty() {
        kprintln!("mkdir: missing directory argument");
        return;
    }
    
    let path = resolve_path(args[0]);
    
    match crate::fs::mkdir(&path) {
        Ok(_) => kprintln!("Created directory: {}", path),
        Err(e) => kprintln!("mkdir: {}: {}", args[0], e),
    }
}

fn cmd_rm(args: &[&str]) {
    if args.is_empty() {
        kprintln!("rm: missing file argument");
        return;
    }
    
    let path = resolve_path(args[0]);
    
    match crate::fs::remove(&path) {
        Ok(_) => kprintln!("Removed: {}", path),
        Err(e) => kprintln!("rm: {}: {}", args[0], e),
    }
}

fn cmd_write(args: &[&str]) {
    if args.len() < 2 {
        kprintln!("write: usage: write <file> <text>");
        return;
    }
    
    let path = resolve_path(args[0]);
    let text = args[1..].join(" ");
    
    // Create file if it doesn't exist
    let inode = match crate::fs::lookup(&path) {
        Ok(i) => i,
        Err(_) => {
            match crate::fs::create(&path) {
                Ok(i) => i,
                Err(e) => {
                    kprintln!("write: cannot create {}: {}", args[0], e);
                    return;
                }
            }
        }
    };
    
    // Write text
    match inode.write(0, text.as_bytes()) {
        Ok(n) => kprintln!("Wrote {} bytes to {}", n, path),
        Err(e) => kprintln!("write: {}: {}", args[0], e),
    }
}

// ==================== DISK FUNCTIONS ====================

const DISK_MAGIC: &[u8; 8] = b"COTTONFS";

/// Initialize disk - check availability (no auto-load, too slow)
fn init_disk() {
    #[cfg(target_arch = "x86_64")]
    {
        use crate::drivers::storage::ata::AtaDevice;
        
        if AtaDevice::detect(0, 0).is_some() {
            set_has_disk(true);
        } else {
            set_has_disk(false);
        }
    }
    
    #[cfg(not(target_arch = "x86_64"))]
    {
        set_has_disk(false);
    }
}