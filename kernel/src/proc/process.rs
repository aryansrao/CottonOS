//! Process Control Block and Management

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use crate::mm::virtual_mem::AddressSpace;

/// Process ID type
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct ProcessId(pub u32);

impl ProcessId {
    pub fn as_u32(&self) -> u32 {
        self.0
    }
}

/// Process state
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ProcessState {
    Created,
    Ready,
    Running,
    Blocked,
    Sleeping,
    Zombie,
}

/// Process priority levels
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Priority {
    Idle = 0,
    Low = 1,
    Normal = 2,
    High = 3,
    Realtime = 4,
}

impl Default for Priority {
    fn default() -> Self {
        Priority::Normal
    }
}

/// CPU context for context switching
#[repr(C)]
#[derive(Clone, Debug, Default)]
pub struct CpuContext {
    #[cfg(target_arch = "x86_64")]
    pub rax: u64,
    #[cfg(target_arch = "x86_64")]
    pub rbx: u64,
    #[cfg(target_arch = "x86_64")]
    pub rcx: u64,
    #[cfg(target_arch = "x86_64")]
    pub rdx: u64,
    #[cfg(target_arch = "x86_64")]
    pub rsi: u64,
    #[cfg(target_arch = "x86_64")]
    pub rdi: u64,
    #[cfg(target_arch = "x86_64")]
    pub rbp: u64,
    #[cfg(target_arch = "x86_64")]
    pub rsp: u64,
    #[cfg(target_arch = "x86_64")]
    pub r8: u64,
    #[cfg(target_arch = "x86_64")]
    pub r9: u64,
    #[cfg(target_arch = "x86_64")]
    pub r10: u64,
    #[cfg(target_arch = "x86_64")]
    pub r11: u64,
    #[cfg(target_arch = "x86_64")]
    pub r12: u64,
    #[cfg(target_arch = "x86_64")]
    pub r13: u64,
    #[cfg(target_arch = "x86_64")]
    pub r14: u64,
    #[cfg(target_arch = "x86_64")]
    pub r15: u64,
    #[cfg(target_arch = "x86_64")]
    pub rip: u64,
    #[cfg(target_arch = "x86_64")]
    pub rflags: u64,
    #[cfg(target_arch = "x86_64")]
    pub cs: u64,
    #[cfg(target_arch = "x86_64")]
    pub ss: u64,
    
    #[cfg(target_arch = "aarch64")]
    pub x: [u64; 31],
    #[cfg(target_arch = "aarch64")]
    pub sp: u64,
    #[cfg(target_arch = "aarch64")]
    pub pc: u64,
    #[cfg(target_arch = "aarch64")]
    pub pstate: u64,
}

/// Process structure
#[derive(Clone)]
pub struct Process {
    /// Process ID
    pub pid: ProcessId,
    /// Parent process ID
    pub parent: Option<ProcessId>,
    /// Process name
    pub name: String,
    /// Process state
    pub state: ProcessState,
    /// Priority
    pub priority: Priority,
    /// CPU context
    pub context: CpuContext,
    /// Virtual address space
    pub address_space: Option<u64>, // Page table root
    /// Kernel stack
    pub kernel_stack: u64,
    /// User stack
    pub user_stack: u64,
    /// Exit status
    pub exit_status: Option<i32>,
    /// Time slice remaining
    pub time_slice: u32,
    /// Total CPU time used (ticks)
    pub cpu_time: u64,
    /// Children processes
    pub children: Vec<ProcessId>,
    /// Open file descriptors
    pub file_descriptors: Vec<Option<usize>>,
    /// Current working directory
    pub cwd: String,
    /// Is kernel process
    pub is_kernel: bool,
}

impl Process {
    /// Create a new kernel process
    pub fn new_kernel(name: &str) -> Option<Self> {
        let pid = super::alloc_pid();
        
        // Allocate kernel stack
        let kernel_stack = crate::mm::physical::alloc_frames(4)?; // 16KB stack
        
        let mut process = Self {
            pid,
            parent: None,
            name: String::from(name),
            state: ProcessState::Created,
            priority: Priority::Normal,
            context: CpuContext::default(),
            address_space: None,
            kernel_stack: kernel_stack + 16384, // Stack grows down
            user_stack: 0,
            exit_status: None,
            time_slice: 10,
            cpu_time: 0,
            children: Vec::new(),
            file_descriptors: vec![None; 256],
            cwd: String::from("/"),
            is_kernel: true,
        };
        
        // Set up initial context
        process.setup_kernel_context();
        
        Some(process)
    }
    
    /// Create a new user process
    pub fn new_user(name: &str, parent: ProcessId) -> Option<Self> {
        let pid = super::alloc_pid();
        
        // Create address space
        let address_space = AddressSpace::new(pid.0)?;
        let page_table_root = address_space.page_table_root;
        
        // Allocate stacks
        let kernel_stack = crate::mm::physical::alloc_frames(4)?;
        let user_stack = crate::mm::physical::alloc_frames(4)?;
        
        let mut process = Self {
            pid,
            parent: Some(parent),
            name: String::from(name),
            state: ProcessState::Created,
            priority: Priority::Normal,
            context: CpuContext::default(),
            address_space: Some(page_table_root),
            kernel_stack: kernel_stack + 16384,
            user_stack: user_stack + 16384,
            exit_status: None,
            time_slice: 10,
            cpu_time: 0,
            children: Vec::new(),
            file_descriptors: vec![None; 256],
            cwd: String::from("/"),
            is_kernel: false,
        };
        
        // Set up initial context for user mode
        process.setup_user_context();
        
        Some(process)
    }
    
    /// Set up kernel mode context
    fn setup_kernel_context(&mut self) {
        #[cfg(target_arch = "x86_64")]
        {
            self.context.rsp = self.kernel_stack;
            self.context.rflags = 0x202; // IF enabled
            self.context.cs = 0x08; // Kernel code segment
            self.context.ss = 0x10; // Kernel data segment
        }
        
        #[cfg(target_arch = "aarch64")]
        {
            self.context.sp = self.kernel_stack;
            self.context.pstate = 0x3C5; // EL1h, interrupts masked
        }
    }
    
    /// Set up user mode context
    fn setup_user_context(&mut self) {
        #[cfg(target_arch = "x86_64")]
        {
            self.context.rsp = 0x7FFF_FFFF_F000; // User stack top
            self.context.rflags = 0x202; // IF enabled
            self.context.cs = 0x1B; // User code segment (ring 3)
            self.context.ss = 0x23; // User data segment (ring 3)
        }
        
        #[cfg(target_arch = "aarch64")]
        {
            self.context.sp = 0x7FFF_FFFF_F000;
            self.context.pstate = 0x0; // EL0
        }
    }
    
    /// Fork this process
    pub fn fork(&self) -> Option<Process> {
        let mut child = if self.is_kernel {
            Self::new_kernel(&self.name)?
        } else {
            Self::new_user(&self.name, self.pid)?
        };
        
        // Copy context
        child.context = self.context.clone();
        child.priority = self.priority;
        child.cwd = self.cwd.clone();
        
        // Copy file descriptors
        child.file_descriptors = self.file_descriptors.clone();
        
        // Add child to parent
        // Note: This should be done by the caller
        
        Some(child)
    }
    
    /// Set entry point
    pub fn set_entry(&mut self, entry: u64) {
        #[cfg(target_arch = "x86_64")]
        {
            self.context.rip = entry;
        }
        
        #[cfg(target_arch = "aarch64")]
        {
            self.context.pc = entry;
        }
    }
    
    /// Set argument
    pub fn set_arg(&mut self, arg: u64) {
        #[cfg(target_arch = "x86_64")]
        {
            self.context.rdi = arg;
        }
        
        #[cfg(target_arch = "aarch64")]
        {
            self.context.x[0] = arg;
        }
    }
}
