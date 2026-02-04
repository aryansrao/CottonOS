//! Virtual Filesystem (VFS) Interface

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use bitflags::bitflags;

/// File type
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum FileType {
    Regular,
    Directory,
    CharDevice,
    BlockDevice,
    Fifo,
    Socket,
    Symlink,
}

bitflags! {
    /// File mode/permissions
    #[derive(Clone, Copy, Debug)]
    pub struct FileMode: u16 {
        const OWNER_READ = 0o400;
        const OWNER_WRITE = 0o200;
        const OWNER_EXEC = 0o100;
        const GROUP_READ = 0o040;
        const GROUP_WRITE = 0o020;
        const GROUP_EXEC = 0o010;
        const OTHER_READ = 0o004;
        const OTHER_WRITE = 0o002;
        const OTHER_EXEC = 0o001;
        
        const OWNER_RWX = Self::OWNER_READ.bits() | Self::OWNER_WRITE.bits() | Self::OWNER_EXEC.bits();
        const GROUP_RWX = Self::GROUP_READ.bits() | Self::GROUP_WRITE.bits() | Self::GROUP_EXEC.bits();
        const OTHER_RWX = Self::OTHER_READ.bits() | Self::OTHER_WRITE.bits() | Self::OTHER_EXEC.bits();
        
        const DEFAULT_FILE = Self::OWNER_READ.bits() | Self::OWNER_WRITE.bits() | Self::GROUP_READ.bits() | Self::OTHER_READ.bits();
        const DEFAULT_DIR = Self::OWNER_RWX.bits() | Self::GROUP_READ.bits() | Self::GROUP_EXEC.bits() | Self::OTHER_READ.bits() | Self::OTHER_EXEC.bits();
    }
}

/// File status
#[derive(Clone, Debug)]
pub struct Stat {
    pub dev: u64,
    pub ino: u64,
    pub mode: FileMode,
    pub nlink: u32,
    pub uid: u32,
    pub gid: u32,
    pub rdev: u64,
    pub size: u64,
    pub blksize: u32,
    pub blocks: u64,
    pub atime: u64,
    pub mtime: u64,
    pub ctime: u64,
    pub file_type: FileType,
}

impl Default for Stat {
    fn default() -> Self {
        Self {
            dev: 0,
            ino: 0,
            mode: FileMode::DEFAULT_FILE,
            nlink: 1,
            uid: 0,
            gid: 0,
            rdev: 0,
            size: 0,
            blksize: 4096,
            blocks: 0,
            atime: 0,
            mtime: 0,
            ctime: 0,
            file_type: FileType::Regular,
        }
    }
}

/// Directory entry
#[derive(Clone, Debug)]
pub struct DirEntry {
    pub name: String,
    pub file_type: FileType,
    pub inode: u64,
}

/// Inode trait - represents a file or directory
pub trait Inode: Send + Sync {
    /// Get inode number
    fn ino(&self) -> u64;
    
    /// Get file type
    fn file_type(&self) -> FileType;
    
    /// Get file status
    fn stat(&self) -> Result<Stat, &'static str>;
    
    /// Read from file
    fn read(&self, offset: u64, buf: &mut [u8]) -> Result<usize, &'static str> {
        Err("Not a regular file")
    }
    
    /// Write to file
    fn write(&self, offset: u64, buf: &[u8]) -> Result<usize, &'static str> {
        Err("Not a regular file")
    }
    
    /// Read directory entries
    fn readdir(&self) -> Result<Vec<DirEntry>, &'static str> {
        Err("Not a directory")
    }
    
    /// Lookup child by name
    fn lookup(&self, name: &str) -> Result<Option<Arc<dyn Inode>>, &'static str> {
        Err("Not a directory")
    }
    
    /// Create file
    fn create(&self, name: &str) -> Result<Arc<dyn Inode>, &'static str> {
        Err("Not a directory")
    }
    
    /// Create directory
    fn mkdir(&self, name: &str) -> Result<Arc<dyn Inode>, &'static str> {
        Err("Not a directory")
    }
    
    /// Remove file
    fn unlink(&self, name: &str) -> Result<(), &'static str> {
        Err("Not a directory")
    }
    
    /// Remove directory
    fn rmdir(&self, name: &str) -> Result<(), &'static str> {
        Err("Not a directory")
    }
    
    /// Rename/move
    fn rename(&self, old_name: &str, new_dir: &Arc<dyn Inode>, new_name: &str) -> Result<(), &'static str> {
        Err("Not a directory")
    }
    
    /// Truncate file
    fn truncate(&self, size: u64) -> Result<(), &'static str> {
        Err("Not a regular file")
    }
    
    /// Change mode
    fn chmod(&self, mode: FileMode) -> Result<(), &'static str> {
        Err("Operation not supported")
    }
    
    /// Change owner
    fn chown(&self, uid: u32, gid: u32) -> Result<(), &'static str> {
        Err("Operation not supported")
    }
    
    /// Sync to disk
    fn sync(&self) -> Result<(), &'static str> {
        Ok(())
    }
    
    /// Device control
    fn ioctl(&self, cmd: u32, arg: u64) -> Result<u64, &'static str> {
        Err("Not a device")
    }
}

/// Filesystem trait
pub trait FileSystem: Send + Sync {
    /// Get filesystem name
    fn name(&self) -> &'static str;
    
    /// Get root inode
    fn root(&self) -> Result<Arc<dyn Inode>, &'static str>;
    
    /// Sync filesystem
    fn sync(&self) -> Result<(), &'static str> {
        Ok(())
    }
    
    /// Get filesystem statistics
    fn statfs(&self) -> Result<FsStats, &'static str> {
        Err("Not implemented")
    }
}

/// Filesystem statistics
#[derive(Clone, Debug)]
pub struct FsStats {
    pub block_size: u32,
    pub total_blocks: u64,
    pub free_blocks: u64,
    pub total_inodes: u64,
    pub free_inodes: u64,
}
