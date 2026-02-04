//! Device Filesystem (devfs)
//!
//! Special filesystem for device files

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use spin::RwLock;
use core::sync::atomic::{AtomicU64, Ordering};

use super::vfs::{DirEntry, FileMode, FileSystem, FileType, Inode, Stat};

/// DevFS filesystem
pub struct DevFS {
    root: Arc<DevDir>,
}

impl DevFS {
    pub fn new() -> Self {
        let root = Arc::new(DevDir::new(1));
        
        // Add standard devices
        {
            let mut entries = root.entries.write();
            
            // /dev/null
            entries.insert(String::from("null"), Arc::new(DevNull::new(2)));
            
            // /dev/zero
            entries.insert(String::from("zero"), Arc::new(DevZero::new(3)));
            
            // /dev/random
            entries.insert(String::from("random"), Arc::new(DevRandom::new(4)));
            
            // /dev/console
            entries.insert(String::from("console"), Arc::new(DevConsole::new(5)));
            
            // /dev/tty
            entries.insert(String::from("tty"), Arc::new(DevTty::new(6)));
        }
        
        Self { root }
    }
}

impl FileSystem for DevFS {
    fn name(&self) -> &'static str {
        "devfs"
    }
    
    fn root(&self) -> Result<Arc<dyn Inode>, &'static str> {
        Ok(self.root.clone())
    }
}

/// Device directory
struct DevDir {
    ino: u64,
    entries: RwLock<BTreeMap<String, Arc<dyn Inode>>>,
}

impl DevDir {
    fn new(ino: u64) -> Self {
        Self {
            ino,
            entries: RwLock::new(BTreeMap::new()),
        }
    }
}

impl Inode for DevDir {
    fn ino(&self) -> u64 {
        self.ino
    }
    
    fn file_type(&self) -> FileType {
        FileType::Directory
    }
    
    fn stat(&self) -> Result<Stat, &'static str> {
        Ok(Stat {
            dev: 0,
            ino: self.ino,
            mode: FileMode::DEFAULT_DIR,
            nlink: 2,
            uid: 0,
            gid: 0,
            rdev: 0,
            size: 0,
            blksize: 4096,
            blocks: 0,
            atime: 0,
            mtime: 0,
            ctime: 0,
            file_type: FileType::Directory,
        })
    }
    
    fn readdir(&self) -> Result<Vec<DirEntry>, &'static str> {
        let entries = self.entries.read();
        let mut result = vec![
            DirEntry {
                name: String::from("."),
                file_type: FileType::Directory,
                inode: self.ino,
            },
            DirEntry {
                name: String::from(".."),
                file_type: FileType::Directory,
                inode: 1,
            },
        ];
        
        for (name, inode) in entries.iter() {
            result.push(DirEntry {
                name: name.clone(),
                file_type: inode.file_type(),
                inode: inode.ino(),
            });
        }
        
        Ok(result)
    }
    
    fn lookup(&self, name: &str) -> Result<Option<Arc<dyn Inode>>, &'static str> {
        if name == "." {
            return Ok(None);
        }
        if name == ".." {
            return Ok(None);
        }
        
        let entries = self.entries.read();
        Ok(entries.get(name).cloned())
    }
}

/// /dev/null device
struct DevNull {
    ino: u64,
}

impl DevNull {
    fn new(ino: u64) -> Self {
        Self { ino }
    }
}

impl Inode for DevNull {
    fn ino(&self) -> u64 {
        self.ino
    }
    
    fn file_type(&self) -> FileType {
        FileType::CharDevice
    }
    
    fn stat(&self) -> Result<Stat, &'static str> {
        Ok(Stat {
            dev: 0,
            ino: self.ino,
            mode: FileMode::OWNER_READ | FileMode::OWNER_WRITE | FileMode::GROUP_READ | FileMode::GROUP_WRITE | FileMode::OTHER_READ | FileMode::OTHER_WRITE,
            nlink: 1,
            uid: 0,
            gid: 0,
            rdev: (1 << 8) | 3, // Major 1, minor 3
            size: 0,
            blksize: 4096,
            blocks: 0,
            atime: 0,
            mtime: 0,
            ctime: 0,
            file_type: FileType::CharDevice,
        })
    }
    
    fn read(&self, _offset: u64, _buf: &mut [u8]) -> Result<usize, &'static str> {
        Ok(0) // Always EOF
    }
    
    fn write(&self, _offset: u64, buf: &[u8]) -> Result<usize, &'static str> {
        Ok(buf.len()) // Discard all data
    }
}

/// /dev/zero device
struct DevZero {
    ino: u64,
}

impl DevZero {
    fn new(ino: u64) -> Self {
        Self { ino }
    }
}

impl Inode for DevZero {
    fn ino(&self) -> u64 {
        self.ino
    }
    
    fn file_type(&self) -> FileType {
        FileType::CharDevice
    }
    
    fn stat(&self) -> Result<Stat, &'static str> {
        Ok(Stat {
            dev: 0,
            ino: self.ino,
            mode: FileMode::OWNER_READ | FileMode::OWNER_WRITE | FileMode::GROUP_READ | FileMode::GROUP_WRITE | FileMode::OTHER_READ | FileMode::OTHER_WRITE,
            nlink: 1,
            uid: 0,
            gid: 0,
            rdev: (1 << 8) | 5, // Major 1, minor 5
            size: 0,
            blksize: 4096,
            blocks: 0,
            atime: 0,
            mtime: 0,
            ctime: 0,
            file_type: FileType::CharDevice,
        })
    }
    
    fn read(&self, _offset: u64, buf: &mut [u8]) -> Result<usize, &'static str> {
        // Fill with zeros
        buf.fill(0);
        Ok(buf.len())
    }
    
    fn write(&self, _offset: u64, buf: &[u8]) -> Result<usize, &'static str> {
        Ok(buf.len()) // Discard all data
    }
}

/// /dev/random device
struct DevRandom {
    ino: u64,
}

impl DevRandom {
    fn new(ino: u64) -> Self {
        Self { ino }
    }
}

impl Inode for DevRandom {
    fn ino(&self) -> u64 {
        self.ino
    }
    
    fn file_type(&self) -> FileType {
        FileType::CharDevice
    }
    
    fn stat(&self) -> Result<Stat, &'static str> {
        Ok(Stat {
            dev: 0,
            ino: self.ino,
            mode: FileMode::OWNER_READ | FileMode::OWNER_WRITE | FileMode::GROUP_READ | FileMode::OTHER_READ,
            nlink: 1,
            uid: 0,
            gid: 0,
            rdev: (1 << 8) | 8, // Major 1, minor 8
            size: 0,
            blksize: 4096,
            blocks: 0,
            atime: 0,
            mtime: 0,
            ctime: 0,
            file_type: FileType::CharDevice,
        })
    }
    
    fn read(&self, _offset: u64, buf: &mut [u8]) -> Result<usize, &'static str> {
        // Simple PRNG - in production use hardware RNG
        static SEED: AtomicU64 = AtomicU64::new(0x12345678);
        
        for byte in buf.iter_mut() {
            let mut s = SEED.load(Ordering::Relaxed);
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            SEED.store(s, Ordering::Relaxed);
            *byte = s as u8;
        }
        
        Ok(buf.len())
    }
    
    fn write(&self, _offset: u64, buf: &[u8]) -> Result<usize, &'static str> {
        // Mix into entropy pool
        Ok(buf.len())
    }
}

/// /dev/console device
struct DevConsole {
    ino: u64,
}

impl DevConsole {
    fn new(ino: u64) -> Self {
        Self { ino }
    }
}

impl Inode for DevConsole {
    fn ino(&self) -> u64 {
        self.ino
    }
    
    fn file_type(&self) -> FileType {
        FileType::CharDevice
    }
    
    fn stat(&self) -> Result<Stat, &'static str> {
        Ok(Stat {
            dev: 0,
            ino: self.ino,
            mode: FileMode::OWNER_READ | FileMode::OWNER_WRITE | FileMode::GROUP_WRITE,
            nlink: 1,
            uid: 0,
            gid: 0,
            rdev: (5 << 8) | 1, // Major 5, minor 1
            size: 0,
            blksize: 4096,
            blocks: 0,
            atime: 0,
            mtime: 0,
            ctime: 0,
            file_type: FileType::CharDevice,
        })
    }
    
    fn read(&self, _offset: u64, _buf: &mut [u8]) -> Result<usize, &'static str> {
        // TODO: Read from keyboard buffer
        Ok(0)
    }
    
    fn write(&self, _offset: u64, buf: &[u8]) -> Result<usize, &'static str> {
        // Write to console
        for &b in buf {
            crate::kprint!("{}", b as char);
        }
        Ok(buf.len())
    }
}

/// /dev/tty device
struct DevTty {
    ino: u64,
}

impl DevTty {
    fn new(ino: u64) -> Self {
        Self { ino }
    }
}

impl Inode for DevTty {
    fn ino(&self) -> u64 {
        self.ino
    }
    
    fn file_type(&self) -> FileType {
        FileType::CharDevice
    }
    
    fn stat(&self) -> Result<Stat, &'static str> {
        Ok(Stat {
            dev: 0,
            ino: self.ino,
            mode: FileMode::OWNER_READ | FileMode::OWNER_WRITE | FileMode::GROUP_WRITE,
            nlink: 1,
            uid: 0,
            gid: 0,
            rdev: (5 << 8) | 0, // Major 5, minor 0
            size: 0,
            blksize: 4096,
            blocks: 0,
            atime: 0,
            mtime: 0,
            ctime: 0,
            file_type: FileType::CharDevice,
        })
    }
    
    fn read(&self, _offset: u64, _buf: &mut [u8]) -> Result<usize, &'static str> {
        Ok(0)
    }
    
    fn write(&self, _offset: u64, buf: &[u8]) -> Result<usize, &'static str> {
        for &b in buf {
            crate::kprint!("{}", b as char);
        }
        Ok(buf.len())
    }
}
