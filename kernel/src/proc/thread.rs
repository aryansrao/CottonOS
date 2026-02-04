//! Thread Management

use alloc::string::String;
use super::process::{ProcessId, CpuContext, Priority};
use core::sync::atomic::{AtomicU64, Ordering};

/// Thread ID type
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct ThreadId(pub u64);

/// Next thread ID
static NEXT_TID: AtomicU64 = AtomicU64::new(1);

/// Allocate new thread ID
pub fn alloc_tid() -> ThreadId {
    ThreadId(NEXT_TID.fetch_add(1, Ordering::SeqCst))
}

/// Thread state
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ThreadState {
    Created,
    Ready,
    Running,
    Blocked,
    Sleeping,
    Terminated,
}

/// Thread structure
#[derive(Clone)]
pub struct Thread {
    /// Thread ID
    pub tid: ThreadId,
    /// Owning process ID
    pub pid: ProcessId,
    /// Thread name
    pub name: String,
    /// Thread state
    pub state: ThreadState,
    /// Priority
    pub priority: Priority,
    /// CPU context
    pub context: CpuContext,
    /// Kernel stack
    pub kernel_stack: u64,
    /// User stack
    pub user_stack: u64,
    /// Time slice remaining
    pub time_slice: u32,
    /// Total CPU time used
    pub cpu_time: u64,
    /// Thread-local storage pointer
    pub tls: u64,
    /// Exit value
    pub exit_value: Option<u64>,
    /// Is main thread
    pub is_main: bool,
}

impl Thread {
    /// Create a new kernel thread
    pub fn new_kernel(pid: ProcessId, name: &str, entry: u64, arg: u64) -> Option<Self> {
        let tid = alloc_tid();
        
        // Allocate kernel stack
        let kernel_stack = crate::mm::physical::alloc_frames(2)?; // 8KB stack
        let stack_top = kernel_stack + 8192;
        
        let mut context = CpuContext::default();
        
        #[cfg(target_arch = "x86_64")]
        {
            context.rsp = stack_top;
            context.rip = entry;
            context.rdi = arg;
            context.rflags = 0x202;
            context.cs = 0x08;
            context.ss = 0x10;
        }
        
        #[cfg(target_arch = "aarch64")]
        {
            context.sp = stack_top;
            context.pc = entry;
            context.x[0] = arg;
            context.pstate = 0x3C5;
        }
        
        Some(Self {
            tid,
            pid,
            name: String::from(name),
            state: ThreadState::Created,
            priority: Priority::Normal,
            context,
            kernel_stack: stack_top,
            user_stack: 0,
            time_slice: 10,
            cpu_time: 0,
            tls: 0,
            exit_value: None,
            is_main: false,
        })
    }
    
    /// Create a new user thread
    pub fn new_user(pid: ProcessId, name: &str, entry: u64, arg: u64, stack: u64) -> Option<Self> {
        let tid = alloc_tid();
        
        // Allocate kernel stack for syscalls
        let kernel_stack = crate::mm::physical::alloc_frames(2)?;
        let kernel_stack_top = kernel_stack + 8192;
        
        let mut context = CpuContext::default();
        
        #[cfg(target_arch = "x86_64")]
        {
            context.rsp = stack;
            context.rip = entry;
            context.rdi = arg;
            context.rflags = 0x202;
            context.cs = 0x1B; // User code
            context.ss = 0x23; // User data
        }
        
        #[cfg(target_arch = "aarch64")]
        {
            context.sp = stack;
            context.pc = entry;
            context.x[0] = arg;
            context.pstate = 0x0; // EL0
        }
        
        Some(Self {
            tid,
            pid,
            name: String::from(name),
            state: ThreadState::Created,
            priority: Priority::Normal,
            context,
            kernel_stack: kernel_stack_top,
            user_stack: stack,
            time_slice: 10,
            cpu_time: 0,
            tls: 0,
            exit_value: None,
            is_main: false,
        })
    }
    
    /// Exit thread
    pub fn exit(&mut self, value: u64) {
        self.exit_value = Some(value);
        self.state = ThreadState::Terminated;
    }
    
    /// Yield thread
    pub fn yield_now(&mut self) {
        if self.state == ThreadState::Running {
            self.state = ThreadState::Ready;
        }
    }
    
    /// Sleep thread
    pub fn sleep(&mut self) {
        self.state = ThreadState::Sleeping;
    }
    
    /// Wake thread
    pub fn wake(&mut self) {
        if self.state == ThreadState::Sleeping || self.state == ThreadState::Blocked {
            self.state = ThreadState::Ready;
        }
    }
    
    /// Block thread
    pub fn block(&mut self) {
        self.state = ThreadState::Blocked;
    }
}

/// Thread-local storage key
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TlsKey(pub u32);

static NEXT_TLS_KEY: AtomicU64 = AtomicU64::new(0);

/// Allocate TLS key
pub fn alloc_tls_key() -> TlsKey {
    TlsKey(NEXT_TLS_KEY.fetch_add(1, Ordering::SeqCst) as u32)
}
