//! Filesystem Module
//!
//! Virtual Filesystem (VFS) layer and CottonFS implementation
//!
//! This module provides:
//! - VFS interface for file operations
//! - CottonFS: The main persistent filesystem
//! - DevFS: Virtual device filesystem
//! - Storage statistics and information

pub mod vfs;
pub mod cottonfs;  // CottonFS - persistent filesystem
pub mod devfs;

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use alloc::collections::BTreeMap;
use spin::RwLock;
use core::sync::atomic::{AtomicU64, Ordering};

pub use vfs::{FileSystem, Inode, DirEntry, FileType, FileMode, Stat, FsStats};
pub use cottonfs::{CottonFS, StorageInfo, get_storage_info};

/// Global VFS root
static VFS_ROOT: RwLock<Option<Arc<dyn Inode>>> = RwLock::new(None);

/// Mounted filesystems
pub static MOUNTS: RwLock<Vec<MountPoint>> = RwLock::new(Vec::new());

/// Mount point
pub struct MountPoint {
    pub path: String,
    pub fs: Arc<dyn FileSystem>,
    pub root: Arc<dyn Inode>,
}

/// Initialize filesystem
/// 
/// This function:
/// 1. Detects available storage devices
/// 2. Creates or mounts the CottonFS filesystem
/// 3. Creates standard directory structure if needed
/// 4. Mounts the DevFS at /dev
pub fn init() {
    crate::kprintln!("[FS] Initializing filesystem...");
    
    // Try to get ATA disk
    let disk = crate::drivers::storage::get_device(0);
    
    let rootfs: Arc<dyn FileSystem> = if let Some(device) = disk {
        crate::kprintln!("[FS] Found disk device, initializing CottonFS...");
        match CottonFS::new(device) {
            Ok(fs) => {
                crate::kprintln!("[FS] CottonFS initialized successfully (persistent storage)");
                fs // Already an Arc
            }
            Err(e) => {
                crate::kprintln!("[FS] Failed to create CottonFS: {}", e);
                crate::kprintln!("[FS] Using RAM-only fallback filesystem");
                Arc::new(RamFS::new())
            }
        }
    } else {
        crate::kprintln!("[FS] No disk found, using RAM-only filesystem");
        Arc::new(RamFS::new())
    };
    
    let root_inode = rootfs.root().expect("Failed to get root inode");
    
    // Set VFS root
    {
        let mut vfs_root = VFS_ROOT.write();
        *vfs_root = Some(root_inode.clone());
    }
    
    // Mount rootfs
    {
        let mut mounts = MOUNTS.write();
        mounts.push(MountPoint {
            path: String::from("/"),
            fs: rootfs.clone(),
            root: root_inode.clone(),
        });
    }
    
    // Create directory structure (only creates if doesn't exist)
    create_directory_structure();
    
    // Create essential system files (only if they don't exist)
    create_system_files();
    
    // Mount devfs at /dev
    let devfs = devfs::DevFS::new();
    if let Err(e) = mount("/dev", Arc::new(devfs)) {
        crate::kprintln!("[FS] Warning: Failed to mount devfs: {}", e);
    }
    
    // Print storage info
    if let Some(info) = get_storage_info() {
        crate::kprintln!("[FS] Storage: {} total, {} used, {} free ({}% used)",
            info.total_display(),
            info.used_display(),
            info.free_display(),
            info.usage_percent()
        );
    }
    
    crate::kprintln!("[FS] Filesystem initialized");
}

/// Create standard directory structure
fn create_directory_structure() {
    crate::kprintln!("[FS] Creating directories...");
    let dirs = [
        "/bin",
        "/dev",
        "/etc",
        "/home",
        "/home/user",
        "/tmp",
        "/var",
        "/var/log",
    ];
    
    for dir in dirs.iter() {
        // Check if already exists
        if lookup(dir).is_ok() {
            continue;
        }
        crate::kprintln!("[FS] Creating {}", dir);
        if let Err(e) = mkdir(dir) {
            crate::kprintln!("[FS] Warning: Failed to create {}: {}", dir, e);
        }
    }
    crate::kprintln!("[FS] Directories created");
}

/// Create essential system files (only if they don't exist)
fn create_system_files() {
    // System configuration - only create if doesn't exist
    if lookup("/etc/hostname").is_err() {
        let _ = write_file("/etc/hostname", b"cottonos");
    }
    if lookup("/etc/version").is_err() {
        let _ = write_file("/etc/version", b"CottonOS 0.1.0");
    }
    // Note: We no longer create welcome.txt - user files persist!
}

/// Get root inode
pub fn root() -> Option<Arc<dyn Inode>> {
    VFS_ROOT.read().clone()
}


/// Mount filesystem at path
pub fn mount(path: &str, fs: Arc<dyn FileSystem>) -> Result<(), &'static str> {
    let root_inode = fs.root()?;
    
    let mut mounts = MOUNTS.write();
    mounts.push(MountPoint {
        path: String::from(path),
        fs,
        root: root_inode,
    });
    
    Ok(())
}

/// Unmount filesystem at path
pub fn umount(path: &str) -> Result<(), &'static str> {
    let mut mounts = MOUNTS.write();
    
    if let Some(pos) = mounts.iter().position(|m| m.path == path) {
        mounts.remove(pos);
        Ok(())
    } else {
        Err("Mount point not found")
    }
}

/// Sync all filesystems
pub fn sync_all() {
    crate::kprintln!("[FS] Syncing all filesystems...");
    let mounts = MOUNTS.read();
    for mount in mounts.iter() {
        if let Err(e) = mount.fs.sync() {
            crate::kprintln!("[FS] Warning: Failed to sync {}: {}", mount.path, e);
        }
    }
    crate::kprintln!("[FS] Sync complete");
}

/// Resolve path to inode
pub fn lookup(path: &str) -> Result<Arc<dyn Inode>, &'static str> {
    if path.is_empty() {
        return Err("Empty path");
    }
    
    let root = root().ok_or("VFS not initialized")?;
    
    if path == "/" {
        return Ok(root);
    }
    
    // Check mount points first
    {
        let mounts = MOUNTS.read();
        for mount in mounts.iter().rev() {
            if path.starts_with(&mount.path) && path != "/" {
                let remaining = &path[mount.path.len()..];
                if remaining.is_empty() || remaining.starts_with('/') {
                    let start = mount.root.clone();
                    if remaining.is_empty() || remaining == "/" {
                        return Ok(start);
                    }
                    return resolve_path(start, &remaining[1..]);
                }
            }
        }
    }
    
    // Resolve from root
    resolve_path(root, &path[1..])
}

/// Resolve relative path from inode
fn resolve_path(start: Arc<dyn Inode>, path: &str) -> Result<Arc<dyn Inode>, &'static str> {
    let mut current = start;
    
    for component in path.split('/') {
        if component.is_empty() || component == "." {
            continue;
        }
        
        if component == ".." {
            // Go to parent
            current = current.lookup("..")?.ok_or("No parent")?;
            continue;
        }
        
        current = current.lookup(component)?.ok_or("Not found")?;
    }
    
    Ok(current)
}

/// Create directory
pub fn mkdir(path: &str) -> Result<Arc<dyn Inode>, &'static str> {
    let (parent_path, name) = split_path(path);
    let parent = lookup(parent_path)?;
    
    parent.mkdir(name)
}

/// Create file
pub fn create(path: &str) -> Result<Arc<dyn Inode>, &'static str> {
    let (parent_path, name) = split_path(path);
    let parent = lookup(parent_path)?;
    
    parent.create(name)
}

/// Remove file or empty directory
pub fn remove(path: &str) -> Result<(), &'static str> {
    let (parent_path, name) = split_path(path);
    let parent = lookup(parent_path)?;
    
    parent.unlink(name)
}

/// Read directory
pub fn readdir(path: &str) -> Result<Vec<DirEntry>, &'static str> {
    let inode = lookup(path)?;
    inode.readdir()
}

/// Get file status
pub fn stat(path: &str) -> Result<Stat, &'static str> {
    let inode = lookup(path)?;
    inode.stat()
}

/// Split path into parent and name
fn split_path(path: &str) -> (&str, &str) {
    if let Some(pos) = path.rfind('/') {
        if pos == 0 {
            ("/", &path[1..])
        } else {
            (&path[..pos], &path[pos + 1..])
        }
    } else {
        (".", path)
    }
}

/// Open file descriptor
pub struct FileDescriptor {
    pub inode: Arc<dyn Inode>,
    pub offset: u64,
    pub flags: u32,
}

impl FileDescriptor {
    pub fn new(inode: Arc<dyn Inode>, flags: u32) -> Self {
        Self {
            inode,
            offset: 0,
            flags,
        }
    }
    
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, &'static str> {
        let n = self.inode.read(self.offset, buf)?;
        self.offset += n as u64;
        Ok(n)
    }
    
    pub fn write(&mut self, buf: &[u8]) -> Result<usize, &'static str> {
        let n = self.inode.write(self.offset, buf)?;
        self.offset += n as u64;
        Ok(n)
    }
    
    pub fn seek(&mut self, offset: u64) {
        self.offset = offset;
    }
}

/// Read entire file contents
pub fn read_file(path: &str) -> Result<Vec<u8>, &'static str> {
    let inode = lookup(path)?;
    let stat = inode.stat()?;
    let size = stat.size as usize;
    
    let mut buffer = Vec::with_capacity(size);
    buffer.resize(size, 0);
    
    let n = inode.read(0, &mut buffer)?;
    buffer.truncate(n);
    
    Ok(buffer)
}

/// Write entire file contents (with auto-sync)
pub fn write_file(path: &str, data: &[u8]) -> Result<(), &'static str> {
    // Try to open existing file or create new one
    let inode = match lookup(path) {
        Ok(inode) => {
            // Truncate existing file
            let _ = inode.truncate(0);
            inode
        }
        Err(_) => create(path)?,
    };
    
    inode.write(0, data)?;
    inode.sync()?; // Sync to disk immediately
    Ok(())
}

// ============================================================================
// RAM-only Fallback Filesystem (used when no disk is available)
// ============================================================================

/// Simple RAM-only filesystem as fallback when no disk is present
pub struct RamFS {
    root: Arc<RamInode>,
    next_ino: AtomicU64,
}

impl RamFS {
    pub fn new() -> Self {
        let root = Arc::new(RamInode::new_dir(1, None));
        Self {
            root,
            next_ino: AtomicU64::new(2),
        }
    }
}

impl FileSystem for RamFS {
    fn name(&self) -> &'static str {
        "ramfs"
    }
    
    fn root(&self) -> Result<Arc<dyn Inode>, &'static str> {
        Ok(self.root.clone())
    }
    
    fn statfs(&self) -> Result<FsStats, &'static str> {
        Ok(FsStats {
            block_size: 4096,
            total_blocks: 1024,
            free_blocks: 512,
            total_inodes: 1024,
            free_inodes: 900,
        })
    }
}

/// Inode data for RAM filesystem
enum RamInodeData {
    File(RwLock<Vec<u8>>),
    Directory(RwLock<BTreeMap<String, Arc<RamInode>>>),
}

/// RAM-based inode
struct RamInode {
    ino: u64,
    file_type: FileType,
    mode: RwLock<FileMode>,
    data: RamInodeData,
    parent: Option<Arc<RamInode>>,
}

impl RamInode {
    fn new_file(ino: u64, _parent: Option<Arc<RamInode>>) -> Self {
        Self {
            ino,
            file_type: FileType::Regular,
            mode: RwLock::new(FileMode::DEFAULT_FILE),
            data: RamInodeData::File(RwLock::new(Vec::new())),
            parent: None,
        }
    }
    
    fn new_dir(ino: u64, parent: Option<Arc<RamInode>>) -> Self {
        Self {
            ino,
            file_type: FileType::Directory,
            mode: RwLock::new(FileMode::DEFAULT_DIR),
            data: RamInodeData::Directory(RwLock::new(BTreeMap::new())),
            parent,
        }
    }
    
    fn get_size(&self) -> u64 {
        match &self.data {
            RamInodeData::File(data) => data.read().len() as u64,
            RamInodeData::Directory(entries) => entries.read().len() as u64 * 32,
        }
    }
}

impl Inode for RamInode {
    fn ino(&self) -> u64 {
        self.ino
    }
    
    fn file_type(&self) -> FileType {
        self.file_type
    }
    
    fn stat(&self) -> Result<Stat, &'static str> {
        Ok(Stat {
            dev: 1,
            ino: self.ino,
            mode: *self.mode.read(),
            nlink: 1,
            uid: 0,
            gid: 0,
            rdev: 0,
            size: self.get_size(),
            blksize: 4096,
            blocks: (self.get_size() + 4095) / 4096,
            atime: 0,
            mtime: 0,
            ctime: 0,
            file_type: self.file_type,
        })
    }
    
    fn read(&self, offset: u64, buf: &mut [u8]) -> Result<usize, &'static str> {
        match &self.data {
            RamInodeData::File(data) => {
                let data = data.read();
                let offset = offset as usize;
                
                if offset >= data.len() {
                    return Ok(0);
                }
                
                let available = data.len() - offset;
                let to_read = buf.len().min(available);
                buf[..to_read].copy_from_slice(&data[offset..offset + to_read]);
                Ok(to_read)
            }
            _ => Err("Not a regular file"),
        }
    }
    
    fn write(&self, offset: u64, buf: &[u8]) -> Result<usize, &'static str> {
        match &self.data {
            RamInodeData::File(data) => {
                let mut data = data.write();
                let offset = offset as usize;
                
                if offset + buf.len() > data.len() {
                    data.resize(offset + buf.len(), 0);
                }
                
                data[offset..offset + buf.len()].copy_from_slice(buf);
                Ok(buf.len())
            }
            _ => Err("Not a regular file"),
        }
    }
    
    fn readdir(&self) -> Result<Vec<DirEntry>, &'static str> {
        match &self.data {
            RamInodeData::Directory(entries) => {
                let entries = entries.read();
                let mut result = Vec::new();
                
                result.push(DirEntry {
                    name: String::from("."),
                    file_type: FileType::Directory,
                    inode: self.ino,
                });
                
                if let Some(ref parent) = self.parent {
                    result.push(DirEntry {
                        name: String::from(".."),
                        file_type: FileType::Directory,
                        inode: parent.ino,
                    });
                } else {
                    result.push(DirEntry {
                        name: String::from(".."),
                        file_type: FileType::Directory,
                        inode: self.ino,
                    });
                }
                
                for (name, inode) in entries.iter() {
                    result.push(DirEntry {
                        name: name.clone(),
                        file_type: inode.file_type,
                        inode: inode.ino,
                    });
                }
                
                Ok(result)
            }
            _ => Err("Not a directory"),
        }
    }
    
    fn lookup(&self, name: &str) -> Result<Option<Arc<dyn Inode>>, &'static str> {
        match &self.data {
            RamInodeData::Directory(entries) => {
                if name == "." {
                    return Ok(None);
                }
                if name == ".." {
                    if let Some(ref parent) = self.parent {
                        return Ok(Some(parent.clone()));
                    }
                    return Ok(None);
                }
                
                let entries = entries.read();
                Ok(entries.get(name).map(|i| i.clone() as Arc<dyn Inode>))
            }
            _ => Err("Not a directory"),
        }
    }
    
    fn create(&self, name: &str) -> Result<Arc<dyn Inode>, &'static str> {
        match &self.data {
            RamInodeData::Directory(entries) => {
                let mut entries = entries.write();
                
                if entries.contains_key(name) {
                    return Err("File exists");
                }
                
                static NEXT_INO: AtomicU64 = AtomicU64::new(1000);
                let ino = NEXT_INO.fetch_add(1, Ordering::SeqCst);
                
                let inode = Arc::new(RamInode::new_file(ino, None));
                entries.insert(String::from(name), inode.clone());
                
                Ok(inode)
            }
            _ => Err("Not a directory"),
        }
    }
    
    fn mkdir(&self, name: &str) -> Result<Arc<dyn Inode>, &'static str> {
        match &self.data {
            RamInodeData::Directory(entries) => {
                let mut entries = entries.write();
                
                if entries.contains_key(name) {
                    return Err("Directory exists");
                }
                
                static NEXT_INO: AtomicU64 = AtomicU64::new(1000);
                let ino = NEXT_INO.fetch_add(1, Ordering::SeqCst);
                
                let inode = Arc::new(RamInode::new_dir(ino, None));
                entries.insert(String::from(name), inode.clone());
                
                Ok(inode)
            }
            _ => Err("Not a directory"),
        }
    }
    
    fn unlink(&self, name: &str) -> Result<(), &'static str> {
        match &self.data {
            RamInodeData::Directory(entries) => {
                let mut entries = entries.write();
                entries.remove(name);
                Ok(())
            }
            _ => Err("Not a directory"),
        }
    }
    
    fn truncate(&self, size: u64) -> Result<(), &'static str> {
        match &self.data {
            RamInodeData::File(data) => {
                let mut data = data.write();
                data.resize(size as usize, 0);
                Ok(())
            }
            _ => Err("Not a regular file"),
        }
    }
}