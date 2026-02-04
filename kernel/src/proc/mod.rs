//! Process Management Module
//!
//! Handles process creation, scheduling, and context switching.

pub mod process;
pub mod scheduler;
pub mod thread;

use alloc::collections::BTreeMap;
use spin::Mutex;

pub use process::{Process, ProcessState, ProcessId};
pub use thread::{Thread, ThreadId, ThreadState};

/// Next available process ID
static NEXT_PID: Mutex<ProcessId> = Mutex::new(ProcessId(1));

/// All processes in the system
static PROCESSES: Mutex<BTreeMap<ProcessId, Process>> = Mutex::new(BTreeMap::new());

/// Initialize process management
pub fn init() {
    // Create init process (PID 1)
    let init = Process::new_kernel("init").expect("Failed to create init process");
    
    let mut processes = PROCESSES.lock();
    processes.insert(init.pid, init);
    
    crate::kprintln!("[PROC] Init process created");
    
    // Initialize scheduler
    scheduler::init();
}

/// Allocate new process ID
pub fn alloc_pid() -> ProcessId {
    let mut pid = NEXT_PID.lock();
    let current = *pid;
    pid.0 += 1;
    current
}

/// Get process by PID
pub fn get_process(pid: ProcessId) -> Option<Process> {
    PROCESSES.lock().get(&pid).cloned()
}

/// Add process to process table
pub fn add_process(process: Process) {
    let pid = process.pid;
    PROCESSES.lock().insert(pid, process);
    scheduler::add_process(pid);
}

/// Remove process from process table
pub fn remove_process(pid: ProcessId) {
    PROCESSES.lock().remove(&pid);
    scheduler::remove_process(pid);
}

/// Get current process
pub fn current() -> Option<Process> {
    let pid = scheduler::current_pid()?;
    get_process(pid)
}

/// Fork current process
pub fn fork() -> Option<ProcessId> {
    let current = current()?;
    let child = current.fork()?;
    let pid = child.pid;
    add_process(child);
    Some(pid)
}

/// Exit current process
pub fn exit(status: i32) {
    if let Some(pid) = scheduler::current_pid() {
        let mut processes = PROCESSES.lock();
        if let Some(process) = processes.get_mut(&pid) {
            process.exit_status = Some(status);
            process.state = ProcessState::Zombie;
        }
    }
    scheduler::schedule();
}

/// Wait for child process
pub fn wait(pid: ProcessId) -> Option<i32> {
    loop {
        {
            let mut processes = PROCESSES.lock();
            if let Some(process) = processes.get(&pid) {
                if process.state == ProcessState::Zombie {
                    let status = process.exit_status;
                    processes.remove(&pid);
                    return status;
                }
            } else {
                return None;
            }
        }
        scheduler::schedule();
    }
}

/// Execute a new program in current process
pub fn exec(_path: &str, _args: &[&str]) -> Result<(), &'static str> {
    // TODO: Load ELF binary, set up address space
    Err("exec not yet implemented")
}

/// Get all process IDs
pub fn all_pids() -> alloc::vec::Vec<ProcessId> {
    PROCESSES.lock().keys().cloned().collect()
}

/// Get process count
pub fn process_count() -> usize {
    PROCESSES.lock().len()
}
