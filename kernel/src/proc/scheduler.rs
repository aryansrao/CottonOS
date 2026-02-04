//! Process Scheduler
//!
//! Round-robin scheduler with priority support

use super::process::{ProcessId, ProcessState};
use alloc::collections::VecDeque;
use spin::Mutex;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Scheduler state
struct Scheduler {
    /// Run queues per priority level
    run_queues: [VecDeque<ProcessId>; 5],
    /// Currently running process
    current: Option<ProcessId>,
    /// Idle process
    idle_pid: Option<ProcessId>,
    /// Is scheduler running
    running: bool,
    /// Tick count
    ticks: u64,
}

impl Scheduler {
    const fn new() -> Self {
        Self {
            run_queues: [
                VecDeque::new(),
                VecDeque::new(),
                VecDeque::new(),
                VecDeque::new(),
                VecDeque::new(),
            ],
            current: None,
            idle_pid: None,
            running: false,
            ticks: 0,
        }
    }
}

/// Global scheduler
static SCHEDULER: Mutex<Scheduler> = Mutex::new(Scheduler::new());

/// Scheduler enabled flag
static SCHEDULER_ENABLED: AtomicBool = AtomicBool::new(false);

/// Total tick count
static TICK_COUNT: AtomicU64 = AtomicU64::new(0);

/// Initialize scheduler
pub fn init() {
    let mut scheduler = SCHEDULER.lock();
    
    // Create idle process
    if let Some(idle) = super::process::Process::new_kernel("idle") {
        scheduler.idle_pid = Some(idle.pid);
        // Don't add idle to run queue, it runs when nothing else can
    }
    
    crate::kprintln!("[SCHED] Scheduler initialized");
}

/// Add process to scheduler
pub fn add_process(pid: ProcessId) {
    let mut scheduler = SCHEDULER.lock();
    
    if let Some(process) = super::get_process(pid) {
        let queue = process.priority as usize;
        scheduler.run_queues[queue].push_back(pid);
    }
}

/// Remove process from scheduler
pub fn remove_process(pid: ProcessId) {
    let mut scheduler = SCHEDULER.lock();
    
    for queue in &mut scheduler.run_queues {
        queue.retain(|&p| p != pid);
    }
    
    if scheduler.current == Some(pid) {
        scheduler.current = None;
    }
}

/// Get current process ID
pub fn current_pid() -> Option<ProcessId> {
    SCHEDULER.lock().current
}

/// Timer tick handler
pub fn timer_tick() {
    TICK_COUNT.fetch_add(1, Ordering::SeqCst);
    
    #[cfg(target_arch = "x86_64")]
    crate::arch::x86_64::pit::tick();
    
    if !SCHEDULER_ENABLED.load(Ordering::SeqCst) {
        return;
    }
    
    let should_schedule = {
        let mut scheduler = SCHEDULER.lock();
        scheduler.ticks += 1;
        
        if let Some(pid) = scheduler.current {
            // Decrement time slice
            let mut processes = super::PROCESSES.lock();
            if let Some(process) = processes.get_mut(&pid) {
                if process.time_slice > 0 {
                    process.time_slice -= 1;
                }
                process.cpu_time += 1;
                process.time_slice == 0
            } else {
                true
            }
        } else {
            true
        }
    };
    
    if should_schedule {
        schedule();
    }
}

/// Select next process to run
fn select_next(scheduler: &mut Scheduler) -> Option<ProcessId> {
    // Check each priority queue from highest to lowest
    for queue in scheduler.run_queues.iter_mut().rev() {
        if let Some(pid) = queue.pop_front() {
            return Some(pid);
        }
    }
    
    // No runnable process, return idle
    scheduler.idle_pid
}

/// Schedule next process
pub fn schedule() {
    if !SCHEDULER_ENABLED.load(Ordering::SeqCst) {
        return;
    }
    
    let (old_pid, new_pid) = {
        let mut scheduler = SCHEDULER.lock();
        
        let old_pid = scheduler.current;
        
        // Put current process back in run queue if still runnable
        if let Some(pid) = old_pid {
            let mut processes = super::PROCESSES.lock();
            if let Some(process) = processes.get_mut(&pid) {
                if process.state == ProcessState::Running {
                    process.state = ProcessState::Ready;
                    process.time_slice = 10; // Reset time slice
                    let queue = process.priority as usize;
                    drop(processes);
                    scheduler.run_queues[queue].push_back(pid);
                }
            }
        }
        
        // Select next process
        let new_pid = select_next(&mut scheduler);
        
        // Update current
        scheduler.current = new_pid;
        
        // Mark new process as running
        if let Some(pid) = new_pid {
            let mut processes = super::PROCESSES.lock();
            if let Some(process) = processes.get_mut(&pid) {
                process.state = ProcessState::Running;
            }
        }
        
        (old_pid, new_pid)
    };
    
    // Perform context switch if needed
    if old_pid != new_pid {
        context_switch(old_pid, new_pid);
    }
}

/// Perform context switch
fn context_switch(old: Option<ProcessId>, new: Option<ProcessId>) {
    let (old_ctx, new_ctx) = {
        let processes = super::PROCESSES.lock();
        
        let old_ctx = old.and_then(|pid| processes.get(&pid).map(|p| p.context.clone()));
        let new_ctx = new.and_then(|pid| processes.get(&pid).map(|p| p.context.clone()));
        
        (old_ctx, new_ctx)
    };
    
    if let Some(new_context) = new_ctx {
        // Save old context and load new
        unsafe {
            do_context_switch(
                old_ctx.as_ref().map(|c| c as *const _).unwrap_or(core::ptr::null()),
                &new_context as *const _
            );
        }
    }
}

/// Architecture-specific context switch
#[cfg(target_arch = "x86_64")]
unsafe fn do_context_switch(old: *const super::process::CpuContext, new: *const super::process::CpuContext) {
    if old.is_null() || new.is_null() {
        return;
    }
    
    // This is a simplified version - full implementation would use assembly
    // to properly save/restore all registers
    core::arch::asm!(
        "mov rsp, {}",
        in(reg) (*new).rsp,
        options(nostack)
    );
}

#[cfg(target_arch = "aarch64")]
unsafe fn do_context_switch(old: *const super::process::CpuContext, new: *const super::process::CpuContext) {
    if old.is_null() || new.is_null() {
        return;
    }
    
    core::arch::asm!(
        "mov sp, {}",
        in(reg) (*new).sp,
        options(nostack)
    );
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
unsafe fn do_context_switch(_old: *const super::process::CpuContext, _new: *const super::process::CpuContext) {
    // Stub for other architectures
}

/// Start the scheduler
pub fn start() -> ! {
    SCHEDULER_ENABLED.store(true, Ordering::SeqCst);
    crate::kprintln!("[SCHED] Scheduler started");
    
    // Enable interrupts
    crate::arch::enable_interrupts();
    
    // Run the kernel shell (interactive mode)
    crate::shell::run()
}

/// Yield current process
pub fn yield_now() {
    schedule();
}

/// Sleep current process for given milliseconds
pub fn sleep_ms(ms: u64) {
    let wake_tick = TICK_COUNT.load(Ordering::SeqCst) + ms;
    
    if let Some(pid) = current_pid() {
        {
            let mut processes = super::PROCESSES.lock();
            if let Some(process) = processes.get_mut(&pid) {
                process.state = ProcessState::Sleeping;
            }
        }
        
        // Simple busy wait for now
        // TODO: Use a proper sleep queue
        while TICK_COUNT.load(Ordering::SeqCst) < wake_tick {
            schedule();
        }
        
        {
            let mut processes = super::PROCESSES.lock();
            if let Some(process) = processes.get_mut(&pid) {
                process.state = ProcessState::Ready;
            }
        }
    }
}

/// Get tick count
pub fn ticks() -> u64 {
    TICK_COUNT.load(Ordering::SeqCst)
}

/// Get scheduler statistics
pub fn stats() -> (usize, usize, u64) {
    let scheduler = SCHEDULER.lock();
    let total_queued: usize = scheduler.run_queues.iter().map(|q| q.len()).sum();
    let running = if scheduler.current.is_some() { 1 } else { 0 };
    (total_queued, running, scheduler.ticks)
}
