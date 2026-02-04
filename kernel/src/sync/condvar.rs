//! Condition Variable

use alloc::collections::VecDeque;
use spin::Mutex;
use crate::proc::{ProcessId, scheduler};
use super::mutex::MutexGuard;

/// Condition variable
pub struct CondVar {
    waiters: Mutex<VecDeque<ProcessId>>,
}

impl CondVar {
    /// Create new condition variable
    pub const fn new() -> Self {
        Self {
            waiters: Mutex::new(VecDeque::new()),
        }
    }
    
    /// Wait on condition, releasing mutex
    pub fn wait<'a, T>(&self, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
        // Add to waiters
        if let Some(pid) = scheduler::current_pid() {
            self.waiters.lock().push_back(pid);
        }
        
        // Release mutex and block
        drop(guard);
        scheduler::yield_now();
        
        // Re-acquire mutex (this is a simplified version)
        // In a real implementation, we'd need to get back the same mutex
        todo!("Need to re-acquire mutex")
    }
    
    /// Notify one waiting thread
    pub fn notify_one(&self) {
        if let Some(_pid) = self.waiters.lock().pop_front() {
            // TODO: Wake specific process
        }
    }
    
    /// Notify all waiting threads
    pub fn notify_all(&self) {
        let mut waiters = self.waiters.lock();
        while let Some(_pid) = waiters.pop_front() {
            // TODO: Wake specific process
        }
    }
    
    /// Check if any threads are waiting
    pub fn has_waiters(&self) -> bool {
        !self.waiters.lock().is_empty()
    }
}

/// Barrier for thread synchronization
pub struct Barrier {
    count: usize,
    current: Mutex<usize>,
    generation: Mutex<usize>,
    waiters: Mutex<VecDeque<ProcessId>>,
}

impl Barrier {
    /// Create barrier for N threads
    pub fn new(count: usize) -> Self {
        Self {
            count,
            current: Mutex::new(0),
            generation: Mutex::new(0),
            waiters: Mutex::new(VecDeque::new()),
        }
    }
    
    /// Wait at barrier
    pub fn wait(&self) -> bool {
        let gen = *self.generation.lock();
        
        let mut current = self.current.lock();
        *current += 1;
        
        if *current == self.count {
            // Last thread - release all
            *current = 0;
            *self.generation.lock() += 1;
            
            // Wake all waiters
            let mut waiters = self.waiters.lock();
            while let Some(_pid) = waiters.pop_front() {
                // TODO: Wake process
            }
            
            return true; // Leader
        }
        
        drop(current);
        
        // Add to waiters
        if let Some(pid) = scheduler::current_pid() {
            self.waiters.lock().push_back(pid);
        }
        
        // Wait for generation change
        loop {
            let new_gen = *self.generation.lock();
            if new_gen != gen {
                break;
            }
            scheduler::yield_now();
        }
        
        false // Follower
    }
}

/// Once - run initialization exactly once
pub struct Once {
    done: core::sync::atomic::AtomicBool,
    running: Mutex<bool>,
}

impl Once {
    pub const fn new() -> Self {
        Self {
            done: core::sync::atomic::AtomicBool::new(false),
            running: Mutex::new(false),
        }
    }
    
    pub fn call_once<F: FnOnce()>(&self, f: F) {
        if self.done.load(core::sync::atomic::Ordering::Acquire) {
            return;
        }
        
        let mut running = self.running.lock();
        
        if self.done.load(core::sync::atomic::Ordering::Acquire) {
            return;
        }
        
        if !*running {
            *running = true;
            drop(running);
            
            f();
            
            self.done.store(true, core::sync::atomic::Ordering::Release);
        } else {
            // Another thread is running initialization
            drop(running);
            while !self.done.load(core::sync::atomic::Ordering::Acquire) {
                scheduler::yield_now();
            }
        }
    }
    
    pub fn is_completed(&self) -> bool {
        self.done.load(core::sync::atomic::Ordering::Acquire)
    }
}
