//! CottonOS Shell
//!
//! Simple command-line shell

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use crate::syscall;

/// Shell instance
pub struct Shell {
    prompt: String,
    history: Vec<String>,
    cwd: String,
}

impl Shell {
    /// Create new shell
    pub fn new() -> Self {
        Self {
            prompt: String::from("cotton$ "),
            history: Vec::new(),
            cwd: String::from("/"),
        }
    }
    
    /// Run shell main loop
    pub fn run(&mut self) {
        syscall::println("Welcome to CottonOS Shell v0.1.0");
        syscall::println("Type 'help' for available commands\n");
        
        loop {
            // Print prompt
            syscall::print(&self.prompt);
            
            // Read command
            let line = self.read_line();
            
            if line.is_empty() {
                continue;
            }
            
            // Add to history
            self.history.push(line.clone());
            
            // Parse and execute
            if !self.execute(&line) {
                break;
            }
        }
    }
    
    /// Read line from stdin
    fn read_line(&self) -> String {
        let mut buf = [0u8; 256];
        let mut line = String::new();
        
        loop {
            let n = syscall::read(0, &mut buf);
            if n <= 0 {
                break;
            }
            
            for &b in &buf[..n as usize] {
                if b == b'\n' || b == b'\r' {
                    syscall::print("\n");
                    return line;
                } else if b == 0x7F || b == 0x08 {
                    // Backspace
                    if !line.is_empty() {
                        line.pop();
                        syscall::print("\x08 \x08");
                    }
                } else if b >= 0x20 {
                    line.push(b as char);
                    syscall::write(1, &[b]);
                }
            }
        }
        
        line
    }
    
    /// Execute command, returns false to exit
    fn execute(&mut self, line: &str) -> bool {
        let parts: Vec<&str> = line.trim().split_whitespace().collect();
        
        if parts.is_empty() {
            return true;
        }
        
        let cmd = parts[0];
        let args = &parts[1..];
        
        match cmd {
            "help" => self.cmd_help(),
            "exit" | "quit" => return false,
            "echo" => self.cmd_echo(args),
            "pwd" => self.cmd_pwd(),
            "cd" => self.cmd_cd(args),
            "ls" => self.cmd_ls(args),
            "mkdir" => self.cmd_mkdir(args),
            "cat" => self.cmd_cat(args),
            "clear" => self.cmd_clear(),
            "uname" => self.cmd_uname(),
            "ps" => self.cmd_ps(),
            "history" => self.cmd_history(),
            "date" => self.cmd_date(),
            "whoami" => syscall::println("root"),
            "hostname" => syscall::println("cotton"),
            _ => {
                syscall::print("Unknown command: ");
                syscall::println(cmd);
            }
        }
        
        true
    }
    
    fn cmd_help(&self) {
        syscall::println("CottonOS Shell Commands:");
        syscall::println("  help     - Show this help message");
        syscall::println("  exit     - Exit the shell");
        syscall::println("  echo     - Print arguments");
        syscall::println("  pwd      - Print working directory");
        syscall::println("  cd       - Change directory");
        syscall::println("  ls       - List directory contents");
        syscall::println("  mkdir    - Create directory");
        syscall::println("  cat      - Show file contents");
        syscall::println("  clear    - Clear screen");
        syscall::println("  uname    - Show system info");
        syscall::println("  ps       - Show processes");
        syscall::println("  history  - Show command history");
        syscall::println("  date     - Show current time");
        syscall::println("  whoami   - Show current user");
        syscall::println("  hostname - Show hostname");
    }
    
    fn cmd_echo(&self, args: &[&str]) {
        for (i, arg) in args.iter().enumerate() {
            if i > 0 {
                syscall::print(" ");
            }
            syscall::print(arg);
        }
        syscall::print("\n");
    }
    
    fn cmd_pwd(&self) {
        syscall::println(&self.cwd);
    }
    
    fn cmd_cd(&mut self, args: &[&str]) {
        let path = if args.is_empty() { "/" } else { args[0] };
        
        let new_path = if path.starts_with('/') {
            String::from(path)
        } else if path == ".." {
            // Go up one level
            if self.cwd == "/" {
                String::from("/")
            } else {
                let mut parts: Vec<&str> = self.cwd.split('/').collect();
                parts.pop();
                if parts.is_empty() || (parts.len() == 1 && parts[0].is_empty()) {
                    String::from("/")
                } else {
                    parts.join("/")
                }
            }
        } else {
            if self.cwd == "/" {
                format!("/{}", path)
            } else {
                format!("{}/{}", self.cwd, path)
            }
        };
        
        // Try to change directory
        let result = syscall::chdir(&new_path);
        if result >= 0 {
            self.cwd = new_path;
        } else {
            syscall::print("cd: ");
            syscall::print(path);
            syscall::println(": No such directory");
        }
    }
    
    fn cmd_ls(&self, args: &[&str]) {
        let path = if args.is_empty() { &self.cwd } else { args[0] };
        
        // TODO: Implement readdir syscall
        syscall::print("Contents of ");
        syscall::println(path);
        syscall::println("  .  (current directory)");
        syscall::println("  .. (parent directory)");
    }
    
    fn cmd_mkdir(&self, args: &[&str]) {
        if args.is_empty() {
            syscall::println("mkdir: missing operand");
            return;
        }
        
        for path in args {
            let full_path = if path.starts_with('/') {
                String::from(*path)
            } else {
                if self.cwd == "/" {
                    format!("/{}", path)
                } else {
                    format!("{}/{}", self.cwd, path)
                }
            };
            
            let result = syscall::mkdir(&full_path);
            if result < 0 {
                syscall::print("mkdir: cannot create directory '");
                syscall::print(path);
                syscall::println("'");
            }
        }
    }
    
    fn cmd_cat(&self, args: &[&str]) {
        if args.is_empty() {
            syscall::println("cat: missing operand");
            return;
        }
        
        for path in args {
            let fd = syscall::open(path, 0);
            if fd < 0 {
                syscall::print("cat: ");
                syscall::print(path);
                syscall::println(": No such file");
                continue;
            }
            
            let mut buf = [0u8; 1024];
            loop {
                let n = syscall::read(fd as usize, &mut buf);
                if n <= 0 {
                    break;
                }
                syscall::write(1, &buf[..n as usize]);
            }
            
            syscall::close(fd as usize);
        }
    }
    
    fn cmd_clear(&self) {
        // ANSI escape sequence to clear screen
        syscall::print("\x1B[2J\x1B[H");
    }
    
    fn cmd_uname(&self) {
        syscall::println("CottonOS 0.1.0 cotton");
    }
    
    fn cmd_ps(&self) {
        syscall::println("  PID TTY          TIME CMD");
        let pid = syscall::getpid();
        syscall::print("    ");
        // Simple integer to string conversion
        if pid == 0 {
            syscall::print("0");
        } else {
            let mut buf = [0u8; 10];
            let mut n = pid;
            let mut i = 0;
            while n > 0 {
                buf[i] = b'0' + (n % 10) as u8;
                n /= 10;
                i += 1;
            }
            for j in (0..i).rev() {
                syscall::write(1, &[buf[j]]);
            }
        }
        syscall::println(" tty0     00:00:00 shell");
    }
    
    fn cmd_history(&self) {
        for (i, cmd) in self.history.iter().enumerate() {
            // Print line number
            let n = i + 1;
            if n < 10 {
                syscall::print("  ");
            } else if n < 100 {
                syscall::print(" ");
            }
            // Simple number printing
            if n >= 100 {
                let d = (n / 100) as u8;
                syscall::write(1, &[b'0' + d]);
            }
            if n >= 10 {
                let d = ((n / 10) % 10) as u8;
                syscall::write(1, &[b'0' + d]);
            }
            let d = (n % 10) as u8;
            syscall::write(1, &[b'0' + d]);
            syscall::print("  ");
            syscall::println(cmd);
        }
    }
    
    fn cmd_date(&self) {
        let time = unsafe { crate::syscall::syscall0(crate::syscall::SYS_TIME) };
        syscall::print("System ticks: ");
        // Print number
        if time == 0 {
            syscall::println("0");
        } else {
            let mut buf = [0u8; 20];
            let mut n = time as u64;
            let mut i = 0;
            while n > 0 {
                buf[i] = b'0' + (n % 10) as u8;
                n /= 10;
                i += 1;
            }
            for j in (0..i).rev() {
                syscall::write(1, &[buf[j]]);
            }
            syscall::print("\n");
        }
    }
}

/// Shell entry point
pub fn main() {
    let mut shell = Shell::new();
    shell.run();
    syscall::exit(0);
}
