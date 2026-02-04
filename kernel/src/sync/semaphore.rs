//! Counting Semaphore

use core::sync::atomic::{AtomicIsize, Ordering};
use alloc::collections::VecDeque;
use spin::Mutex;
use crate::proc::{ProcessId, scheduler};

/// Counting semaphore
pub struct Semaphore {
    count: AtomicIsize,
    waiters: Mutex<VecDeque<ProcessId>>,
}

impl Semaphore {
    /// Create new semaphore with initial count
    pub const fn new(count: isize) -> Self {
        Self {
            count: AtomicIsize::new(count),
            waiters: Mutex::new(VecDeque::new()),
        }
    }
    
    /// Wait (P operation / down)
    pub fn wait(&self) {
        loop {
            let count = self.count.load(Ordering::Acquire);
            
            if count > 0 {
                if self.count.compare_exchange(count, count - 1, Ordering::AcqRel, Ordering::Relaxed).is_ok() {
                    return;
                }
            } else {
                // Add to waiters
                if let Some(pid) = scheduler::current_pid() {
                    self.waiters.lock().push_back(pid);
                }
                scheduler::yield_now();
            }
        }
    }
    
    /// Try wait without blocking
    pub fn try_wait(&self) -> bool {
        loop {
            let count = self.count.load(Ordering::Acquire);
            
            if count > 0 {
                if self.count.compare_exchange(count, count - 1, Ordering::AcqRel, Ordering::Relaxed).is_ok() {
                    return true;
                }
            } else {
                return false;
            }
        }
    }
    
    /// Signal (V operation / up)
    pub fn signal(&self) {
        self.count.fetch_add(1, Ordering::Release);
        
        // Wake one waiter
        if let Some(_pid) = self.waiters.lock().pop_front() {
            // TODO: Wake specific process
        }
    }
    
    /// Get current count
    pub fn count(&self) -> isize {
        self.count.load(Ordering::Relaxed)
    }
}

/// Binary semaphore (mutex-like)
pub struct BinarySemaphore {
    inner: Semaphore,
}

impl BinarySemaphore {
    pub const fn new(available: bool) -> Self {
        Self {
            inner: Semaphore::new(if available { 1 } else { 0 }),
        }
    }
    
    pub fn acquire(&self) {
        self.inner.wait();
    }
    
    pub fn try_acquire(&self) -> bool {
        self.inner.try_wait()
    }
    
    pub fn release(&self) {
        // Ensure count doesn't exceed 1
        let count = self.inner.count.load(Ordering::Acquire);
        if count < 1 {
            self.inner.signal();
        }
    }
}
