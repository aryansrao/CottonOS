//! Mutex (Mutual Exclusion Lock)

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use core::ops::{Deref, DerefMut};
use alloc::collections::VecDeque;
use spin::Mutex as SpinMutex;
use crate::proc::{ProcessId, scheduler};

/// Mutex that can block waiting threads
pub struct Mutex<T> {
    locked: AtomicBool,
    owner: AtomicU32,
    waiters: SpinMutex<VecDeque<ProcessId>>,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for Mutex<T> {}
unsafe impl<T: Send> Send for Mutex<T> {}

impl<T> Mutex<T> {
    /// Create new mutex
    pub const fn new(data: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            owner: AtomicU32::new(0),
            waiters: SpinMutex::new(VecDeque::new()),
            data: UnsafeCell::new(data),
        }
    }
    
    /// Try to acquire the mutex without blocking
    pub fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
        if self.locked.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {
            if let Some(current) = crate::proc::scheduler::current_pid() {
                self.owner.store(current.as_u32(), Ordering::Relaxed);
            }
            Some(MutexGuard { mutex: self })
        } else {
            None
        }
    }
    
    /// Acquire the mutex, blocking if necessary
    pub fn lock(&self) -> MutexGuard<'_, T> {
        loop {
            if let Some(guard) = self.try_lock() {
                return guard;
            }
            
            // Add to waiters and block
            if let Some(pid) = scheduler::current_pid() {
                self.waiters.lock().push_back(pid);
            }
            
            scheduler::yield_now();
        }
    }
    
    /// Check if mutex is locked
    pub fn is_locked(&self) -> bool {
        self.locked.load(Ordering::Relaxed)
    }
    
    fn unlock(&self) {
        self.owner.store(0, Ordering::Relaxed);
        self.locked.store(false, Ordering::Release);
        
        // Wake one waiter
        if let Some(_pid) = self.waiters.lock().pop_front() {
            // TODO: Wake specific process
        }
    }
}

/// Mutex guard (RAII)
pub struct MutexGuard<'a, T> {
    mutex: &'a Mutex<T>,
}

impl<T> Deref for MutexGuard<'_, T> {
    type Target = T;
    
    fn deref(&self) -> &T {
        unsafe { &*self.mutex.data.get() }
    }
}

impl<T> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.mutex.data.get() }
    }
}

impl<T> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        self.mutex.unlock();
    }
}

/// Recursive mutex
pub struct RecursiveMutex<T> {
    locked: AtomicBool,
    owner: AtomicU32,
    count: AtomicU32,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for RecursiveMutex<T> {}
unsafe impl<T: Send> Send for RecursiveMutex<T> {}

impl<T> RecursiveMutex<T> {
    pub const fn new(data: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            owner: AtomicU32::new(0),
            count: AtomicU32::new(0),
            data: UnsafeCell::new(data),
        }
    }
    
    pub fn lock(&self) -> RecursiveMutexGuard<'_, T> {
        let current_pid = scheduler::current_pid()
            .map(|p| p.as_u32())
            .unwrap_or(0);
        
        loop {
            // Check if we already own it
            if self.locked.load(Ordering::Acquire) {
                if self.owner.load(Ordering::Relaxed) == current_pid {
                    self.count.fetch_add(1, Ordering::Relaxed);
                    return RecursiveMutexGuard { mutex: self };
                }
            }
            
            // Try to acquire
            if self.locked.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {
                self.owner.store(current_pid, Ordering::Relaxed);
                self.count.store(1, Ordering::Relaxed);
                return RecursiveMutexGuard { mutex: self };
            }
            
            scheduler::yield_now();
        }
    }
    
    fn unlock(&self) {
        let count = self.count.fetch_sub(1, Ordering::Relaxed);
        if count == 1 {
            self.owner.store(0, Ordering::Relaxed);
            self.locked.store(false, Ordering::Release);
        }
    }
}

pub struct RecursiveMutexGuard<'a, T> {
    mutex: &'a RecursiveMutex<T>,
}

impl<T> Deref for RecursiveMutexGuard<'_, T> {
    type Target = T;
    
    fn deref(&self) -> &T {
        unsafe { &*self.mutex.data.get() }
    }
}

impl<T> DerefMut for RecursiveMutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.mutex.data.get() }
    }
}

impl<T> Drop for RecursiveMutexGuard<'_, T> {
    fn drop(&mut self) {
        self.mutex.unlock();
    }
}

/// Reader-writer lock
pub struct RwLock<T> {
    readers: AtomicU32,
    writer: AtomicBool,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send + Sync> Sync for RwLock<T> {}
unsafe impl<T: Send> Send for RwLock<T> {}

impl<T> RwLock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            readers: AtomicU32::new(0),
            writer: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }
    
    pub fn read(&self) -> RwLockReadGuard<'_, T> {
        loop {
            // Wait for no writer
            while self.writer.load(Ordering::Acquire) {
                scheduler::yield_now();
            }
            
            // Try to increment readers
            let _readers = self.readers.fetch_add(1, Ordering::Acquire);
            
            // Check if writer snuck in
            if self.writer.load(Ordering::Acquire) {
                self.readers.fetch_sub(1, Ordering::Release);
                continue;
            }
            
            return RwLockReadGuard { lock: self };
        }
    }
    
    pub fn write(&self) -> RwLockWriteGuard<'_, T> {
        loop {
            // Try to become writer
            if self.writer.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {
                // Wait for readers to drain
                while self.readers.load(Ordering::Acquire) > 0 {
                    scheduler::yield_now();
                }
                return RwLockWriteGuard { lock: self };
            }
            
            scheduler::yield_now();
        }
    }
}

pub struct RwLockReadGuard<'a, T> {
    lock: &'a RwLock<T>,
}

impl<T> Deref for RwLockReadGuard<'_, T> {
    type Target = T;
    
    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> Drop for RwLockReadGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.readers.fetch_sub(1, Ordering::Release);
    }
}

pub struct RwLockWriteGuard<'a, T> {
    lock: &'a RwLock<T>,
}

impl<T> Deref for RwLockWriteGuard<'_, T> {
    type Target = T;
    
    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> DerefMut for RwLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T> Drop for RwLockWriteGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.writer.store(false, Ordering::Release);
    }
}
