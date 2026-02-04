//! CottonFS - Custom Filesystem for CottonOS
//!
//! A simple, safe, and persistent filesystem with proper disk storage.
//! 
//! ## Disk Layout (4KB blocks)
//! ```text
//! Block 0:     Superblock (filesystem metadata)
//! Block 1-31:  Inode bitmap (tracks which inodes are allocated)
//! Block 32-63: Data bitmap (tracks which data blocks are used)
//! Block 64-127: Inode table (stores all inode metadata)
//! Block 128+:  Data blocks (actual file/directory content)
//! ```
//!
//! ## Design Goals
//! - Simple and easy to understand
//! - Safe concurrent access via Mutex
//! - Persistent storage with immediate sync
//! - Accurate storage statistics

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use spin::{Mutex, RwLock};
use core::sync::atomic::{AtomicU64, Ordering};

use super::vfs::{DirEntry, FileMode, FileSystem, FileType, FsStats, Inode, Stat};
use crate::drivers::storage::BlockDevice;

// ============================================================================
// Constants
// ============================================================================

/// Block size in bytes (4KB)
pub const BLOCK_SIZE: usize = 4096;

/// Sectors per block (512-byte sectors)
const SECTORS_PER_BLOCK: u64 = 8;

/// Magic number identifying CottonFS ("CTFS" in hex)
const FS_MAGIC: u32 = 0x43544653;

/// Filesystem version
const FS_VERSION: u32 = 2;

// Block layout
const SUPERBLOCK_BLOCK: u64 = 0;
const INODE_BITMAP_START: u64 = 1;
const INODE_BITMAP_BLOCKS: u64 = 31;
const DATA_BITMAP_START: u64 = 32;
const DATA_BITMAP_BLOCKS: u64 = 32;
const INODE_TABLE_START: u64 = 64;
const INODE_TABLE_BLOCKS: u64 = 64;
const DATA_BLOCKS_START: u64 = 128;

/// Maximum number of inodes (limited by inode table size)
const MAX_INODES: u64 = (INODE_TABLE_BLOCKS * BLOCK_SIZE as u64) / DISK_INODE_SIZE as u64;

/// Size of on-disk inode structure
const DISK_INODE_SIZE: usize = 128;

/// Maximum filename length
const MAX_FILENAME: usize = 60;

/// Maximum file size (using direct + single indirect blocks)
/// 12 direct blocks + 1024 indirect = ~4MB per file
const DIRECT_BLOCKS: usize = 12;

/// Root inode number (always 1)
const ROOT_INODE: u64 = 1;

// ============================================================================
// On-Disk Structures
// ============================================================================

/// Superblock - stored at block 0
#[repr(C)]
#[derive(Clone, Copy)]
struct Superblock {
    magic: u32,              // Magic number (FS_MAGIC)
    version: u32,            // Filesystem version
    block_size: u32,         // Block size in bytes
    total_blocks: u64,       // Total blocks on disk
    total_inodes: u64,       // Total inode slots
    free_blocks: u64,        // Number of free data blocks
    free_inodes: u64,        // Number of free inodes
    root_inode: u64,         // Root directory inode number
    mount_count: u32,        // Number of times mounted
    last_mount_time: u64,    // Last mount timestamp
    _reserved: [u8; 64],     // Reserved for future use
}

impl Superblock {
    fn new(total_blocks: u64) -> Self {
        let data_blocks = total_blocks.saturating_sub(DATA_BLOCKS_START);
        Self {
            magic: FS_MAGIC,
            version: FS_VERSION,
            block_size: BLOCK_SIZE as u32,
            total_blocks,
            total_inodes: MAX_INODES,
            free_blocks: data_blocks,
            free_inodes: MAX_INODES - 1, // Root inode is allocated
            root_inode: ROOT_INODE,
            mount_count: 1,
            last_mount_time: 0,
            _reserved: [0; 64],
        }
    }
}

/// On-disk inode structure (128 bytes)
#[repr(C)]
#[derive(Clone, Copy)]
struct DiskInode {
    mode: u16,               // File mode/permissions
    file_type: u8,           // 0=free, 1=file, 2=directory, 3=symlink
    _pad1: u8,
    uid: u32,                // Owner user ID
    gid: u32,                // Owner group ID
    size: u64,               // File size in bytes
    blocks: u64,             // Number of blocks allocated
    atime: u64,              // Access time
    mtime: u64,              // Modification time
    ctime: u64,              // Creation time
    nlink: u32,              // Number of hard links
    _pad2: u32,
    direct: [u64; DIRECT_BLOCKS], // Direct block pointers
    indirect: u64,           // Single indirect block pointer
}

impl DiskInode {
    fn new_file() -> Self {
        Self {
            mode: FileMode::DEFAULT_FILE.bits(),
            file_type: 1,
            _pad1: 0,
            uid: 0,
            gid: 0,
            size: 0,
            blocks: 0,
            atime: 0,
            mtime: 0,
            ctime: 0,
            nlink: 1,
            _pad2: 0,
            direct: [0; DIRECT_BLOCKS],
            indirect: 0,
        }
    }

    fn new_dir() -> Self {
        Self {
            mode: FileMode::DEFAULT_DIR.bits(),
            file_type: 2,
            _pad1: 0,
            uid: 0,
            gid: 0,
            size: 0,
            blocks: 0,
            atime: 0,
            mtime: 0,
            ctime: 0,
            nlink: 2, // . and parent link
            _pad2: 0,
            direct: [0; DIRECT_BLOCKS],
            indirect: 0,
        }
    }

    fn is_free(&self) -> bool {
        self.file_type == 0
    }

    fn get_file_type(&self) -> FileType {
        match self.file_type {
            1 => FileType::Regular,
            2 => FileType::Directory,
            3 => FileType::Symlink,
            _ => FileType::Regular,
        }
    }
}

/// Directory entry on disk (64 bytes)
#[repr(C)]
#[derive(Clone, Copy)]
struct DiskDirEntry {
    inode: u64,                    // Inode number (0 = empty slot)
    name_len: u8,                  // Length of filename
    file_type: u8,                 // File type for quick access
    _pad: [u8; 2],
    name: [u8; MAX_FILENAME],      // Filename (null-padded)
}

impl DiskDirEntry {
    fn new(inode: u64, name: &str, file_type: FileType) -> Self {
        let mut entry = Self {
            inode,
            name_len: name.len().min(MAX_FILENAME) as u8,
            file_type: match file_type {
                FileType::Regular => 1,
                FileType::Directory => 2,
                FileType::Symlink => 3,
                _ => 0,
            },
            _pad: [0; 2],
            name: [0; MAX_FILENAME],
        };
        let bytes = name.as_bytes();
        let len = bytes.len().min(MAX_FILENAME);
        entry.name[..len].copy_from_slice(&bytes[..len]);
        entry
    }

    fn get_name(&self) -> String {
        let len = self.name_len as usize;
        String::from_utf8_lossy(&self.name[..len]).into_owned()
    }

    fn is_empty(&self) -> bool {
        self.inode == 0
    }
}

// ============================================================================
// CottonFS - Main Filesystem
// ============================================================================

/// CottonFS filesystem
pub struct CottonFS {
    /// Block device for storage
    device: Arc<dyn BlockDevice>,
    /// Cached superblock (use Mutex for simpler locking)
    superblock: Mutex<Superblock>,
    /// Inode bitmap cache
    inode_bitmap: Mutex<Vec<u8>>,
    /// Data block bitmap cache
    data_bitmap: Mutex<Vec<u8>>,
    /// In-memory inode cache
    inode_cache: RwLock<BTreeMap<u64, Arc<CottonInode>>>,
    /// Root inode
    root: Arc<CottonInode>,
}

impl CottonFS {
    /// Create or mount filesystem on the given block device
    /// Returns an Arc to ensure the Mutex doesn't move after creation
    pub fn new(device: Arc<dyn BlockDevice>) -> Result<Arc<Self>, &'static str> {
        crate::kprintln!("[CottonFS] Initializing filesystem...");
        
        // Read superblock
        crate::kprintln!("[CottonFS] Reading superblock...");
        let mut buf = vec![0u8; BLOCK_SIZE];
        read_block(&device, SUPERBLOCK_BLOCK, &mut buf)?;
        crate::kprintln!("[CottonFS] Superblock read OK");
        
        let superblock: Superblock = unsafe {
            core::ptr::read(buf.as_ptr() as *const Superblock)
        };
        
        // Check if we have a valid filesystem
        let (superblock, needs_format) = if superblock.magic == FS_MAGIC && superblock.version == FS_VERSION {
            crate::kprintln!("[CottonFS] Found existing filesystem (v{})", superblock.version);
            crate::kprintln!("[CottonFS]   Total blocks: {}", superblock.total_blocks);
            crate::kprintln!("[CottonFS]   Free blocks: {}", superblock.free_blocks);
            crate::kprintln!("[CottonFS]   Free inodes: {}", superblock.free_inodes);
            (superblock, false)
        } else {
            crate::kprintln!("[CottonFS] No valid filesystem found, formatting...");
            let sb = Superblock::new(device.total_blocks());
            (sb, true)
        };
        
        // Read or initialize bitmaps
        let inode_bitmap_size = (INODE_BITMAP_BLOCKS as usize) * BLOCK_SIZE;
        let data_bitmap_size = (DATA_BITMAP_BLOCKS as usize) * BLOCK_SIZE;
        
        let mut inode_bitmap = vec![0u8; inode_bitmap_size];
        let mut data_bitmap = vec![0u8; data_bitmap_size];
        
        if !needs_format {
            // Read existing bitmaps
            for i in 0..INODE_BITMAP_BLOCKS {
                let offset = (i as usize) * BLOCK_SIZE;
                read_block(&device, INODE_BITMAP_START + i, &mut inode_bitmap[offset..offset + BLOCK_SIZE])?;
            }
            for i in 0..DATA_BITMAP_BLOCKS {
                let offset = (i as usize) * BLOCK_SIZE;
                read_block(&device, DATA_BITMAP_START + i, &mut data_bitmap[offset..offset + BLOCK_SIZE])?;
            }
        } else {
            // Mark root inode as allocated
            set_bit(&mut inode_bitmap, ROOT_INODE as usize);
        }
        
        // Create filesystem in Arc immediately to prevent moving
        // The Mutex must not be moved after creation!
        let fs = Arc::new(Self {
            device: device,
            superblock: Mutex::new(superblock),
            inode_bitmap: Mutex::new(inode_bitmap),
            data_bitmap: Mutex::new(data_bitmap),
            inode_cache: RwLock::new(BTreeMap::new()),
            root: Arc::new(CottonInode::new_placeholder(ROOT_INODE)), // Temporary placeholder
        });
        
        // Format if needed (uses the Mutex through &self)
        if needs_format {
            fs.format()?;
        }
        
        // Load root inode
        let root = fs.load_inode_internal(ROOT_INODE)?;
        
        // We need to update the root field - since Arc doesn't allow mutation,
        // we use unsafe to update it. This is safe because we're the only owner
        // and the placeholder was never used.
        unsafe {
            let fs_mut = Arc::as_ptr(&fs) as *mut Self;
            (*fs_mut).root = root;
        }
        
        crate::kprintln!("[CottonFS] Filesystem ready");
        Ok(fs)
    }
    
    /// Load an inode - internal version that doesn't use self.root
    fn load_inode_internal(&self, ino: u64) -> Result<Arc<CottonInode>, &'static str> {
        // Check cache first
        {
            let cache = self.inode_cache.read();
            if let Some(inode) = cache.get(&ino) {
                return Ok(inode.clone());
            }
        }
        
        // Read from disk
        let disk_inode = self.read_disk_inode(ino)?;
        
        if disk_inode.is_free() {
            return Err("Inode is not allocated");
        }
        
        let inode = Arc::new(CottonInode {
            ino,
            fs: self as *const CottonFS,
            file_type: disk_inode.get_file_type(),
            disk_inode: RwLock::new(disk_inode),
            dir_entries: RwLock::new(None),
            file_data: RwLock::new(None),
            dirty: AtomicU64::new(0),
        });
        
        // Cache it
        {
            let mut cache = self.inode_cache.write();
            cache.insert(ino, inode.clone());
        }
        
        Ok(inode)
    }
    
    /// Format the filesystem (create empty root directory)
    fn format(&self) -> Result<(), &'static str> {
        crate::kprintln!("[CottonFS] Formatting filesystem...");
        
        // Write superblock
        self.sync_superblock()?;
        
        // Write empty inode bitmap (with root inode marked)
        self.sync_inode_bitmap()?;
        
        // Write empty data bitmap
        self.sync_data_bitmap()?;
        
        // Create root inode
        let root_disk_inode = DiskInode::new_dir();
        self.write_disk_inode(ROOT_INODE, &root_disk_inode)?;
        
        crate::kprintln!("[CottonFS] Format complete");
        Ok(())
    }
    
    /// Load an inode from disk or cache (public version)
    fn load_inode(&self, ino: u64) -> Result<Arc<CottonInode>, &'static str> {
        self.load_inode_internal(ino)
    }
    
    /// Allocate a new inode
    fn alloc_inode(&self) -> Result<u64, &'static str> {
        let mut bitmap = self.inode_bitmap.lock();
        let mut sb = self.superblock.lock();
        
        if sb.free_inodes == 0 {
            return Err("No free inodes");
        }
        
        // Find first free inode (skip 0, start from 1)
        for i in 1..(MAX_INODES as usize) {
            if !get_bit(&bitmap, i) {
                set_bit(&mut bitmap, i);
                sb.free_inodes -= 1;
                
                // Drop locks before disk I/O
                drop(bitmap);
                drop(sb);
                
                // Sync to disk
                self.sync_inode_bitmap()?;
                self.sync_superblock()?;
                
                return Ok(i as u64);
            }
        }
        
        Err("No free inodes")
    }
    
    /// Free an inode
    fn free_inode(&self, ino: u64) -> Result<(), &'static str> {
        if ino == ROOT_INODE {
            return Err("Cannot free root inode");
        }
        
        let mut bitmap = self.inode_bitmap.lock();
        let mut sb = self.superblock.lock();
        
        clear_bit(&mut bitmap, ino as usize);
        sb.free_inodes += 1;
        
        // Remove from cache
        {
            let mut cache = self.inode_cache.write();
            cache.remove(&ino);
        }
        
        drop(bitmap);
        drop(sb);
        self.sync_inode_bitmap()?;
        self.sync_superblock()?;
        
        Ok(())
    }
    
    /// Allocate a data block
    fn alloc_block(&self) -> Result<u64, &'static str> {
        let mut bitmap = self.data_bitmap.lock();
        let mut sb = self.superblock.lock();
        
        if sb.free_blocks == 0 {
            return Err("No free blocks");
        }
        
        let max_blocks = sb.total_blocks.saturating_sub(DATA_BLOCKS_START) as usize;
        
        for i in 0..max_blocks {
            if !get_bit(&bitmap, i) {
                set_bit(&mut bitmap, i);
                sb.free_blocks -= 1;
                
                drop(bitmap);
                drop(sb);
                self.sync_data_bitmap()?;
                self.sync_superblock()?;
                
                return Ok(DATA_BLOCKS_START + i as u64);
            }
        }
        
        Err("No free blocks")
    }
    
    /// Free a data block
    fn free_block(&self, block: u64) -> Result<(), &'static str> {
        if block < DATA_BLOCKS_START {
            return Err("Invalid block number");
        }
        
        let index = (block - DATA_BLOCKS_START) as usize;
        
        let mut bitmap = self.data_bitmap.lock();
        let mut sb = self.superblock.lock();
        
        if !get_bit(&bitmap, index) {
            return Ok(()); // Already free
        }
        
        clear_bit(&mut bitmap, index);
        sb.free_blocks += 1;
        
        drop(bitmap);
        drop(sb);
        self.sync_data_bitmap()?;
        self.sync_superblock()?;
        
        Ok(())
    }
    
    /// Read disk inode
    fn read_disk_inode(&self, ino: u64) -> Result<DiskInode, &'static str> {
        let inodes_per_block = BLOCK_SIZE / DISK_INODE_SIZE;
        let block = INODE_TABLE_START + (ino as u64 / inodes_per_block as u64);
        let offset = (ino as usize % inodes_per_block) * DISK_INODE_SIZE;
        
        let mut buf = vec![0u8; BLOCK_SIZE];
        read_block(&self.device, block, &mut buf)?;
        
        let inode: DiskInode = unsafe {
            core::ptr::read(buf[offset..].as_ptr() as *const DiskInode)
        };
        
        Ok(inode)
    }
    
    /// Write disk inode
    fn write_disk_inode(&self, ino: u64, inode: &DiskInode) -> Result<(), &'static str> {
        let inodes_per_block = BLOCK_SIZE / DISK_INODE_SIZE;
        let block = INODE_TABLE_START + (ino as u64 / inodes_per_block as u64);
        let offset = (ino as usize % inodes_per_block) * DISK_INODE_SIZE;
        
        let mut buf = vec![0u8; BLOCK_SIZE];
        read_block(&self.device, block, &mut buf)?;
        
        let inode_bytes = unsafe {
            core::slice::from_raw_parts(inode as *const DiskInode as *const u8, DISK_INODE_SIZE)
        };
        buf[offset..offset + DISK_INODE_SIZE].copy_from_slice(inode_bytes);
        
        write_block(&self.device, block, &buf)?;
        Ok(())
    }
    
    /// Sync superblock to disk
    fn sync_superblock(&self) -> Result<(), &'static str> {
        let sb = self.superblock.lock();
        let mut buf = vec![0u8; BLOCK_SIZE];
        
        let sb_bytes = unsafe {
            core::slice::from_raw_parts(&*sb as *const Superblock as *const u8, core::mem::size_of::<Superblock>())
        };
        buf[..sb_bytes.len()].copy_from_slice(sb_bytes);
        
        drop(sb); // Release lock before I/O
        write_block(&self.device, SUPERBLOCK_BLOCK, &buf)?;
        Ok(())
    }
    
    /// Sync inode bitmap to disk
    fn sync_inode_bitmap(&self) -> Result<(), &'static str> {
        let bitmap = self.inode_bitmap.lock();
        // Copy bitmap data while holding lock
        let bitmap_data: Vec<u8> = bitmap.clone();
        drop(bitmap); // Release lock before I/O
        
        for i in 0..INODE_BITMAP_BLOCKS {
            let offset = (i as usize) * BLOCK_SIZE;
            write_block(&self.device, INODE_BITMAP_START + i, &bitmap_data[offset..offset + BLOCK_SIZE])?;
        }
        Ok(())
    }
    
    /// Sync data bitmap to disk
    fn sync_data_bitmap(&self) -> Result<(), &'static str> {
        let bitmap = self.data_bitmap.lock();
        // Copy bitmap data while holding lock
        let bitmap_data: Vec<u8> = bitmap.clone();
        drop(bitmap); // Release lock before I/O
        
        for i in 0..DATA_BITMAP_BLOCKS {
            let offset = (i as usize) * BLOCK_SIZE;
            write_block(&self.device, DATA_BITMAP_START + i, &bitmap_data[offset..offset + BLOCK_SIZE])?;
        }
        Ok(())
    }
    
    /// Get filesystem statistics
    pub fn get_stats(&self) -> FsStats {
        let sb = self.superblock.lock();
        FsStats {
            block_size: sb.block_size,
            total_blocks: sb.total_blocks.saturating_sub(DATA_BLOCKS_START),
            free_blocks: sb.free_blocks,
            total_inodes: sb.total_inodes,
            free_inodes: sb.free_inodes,
        }
    }
    
    /// Get storage usage information
    pub fn get_storage_info(&self) -> StorageInfo {
        let sb = self.superblock.lock();
        let total_data_blocks = sb.total_blocks.saturating_sub(DATA_BLOCKS_START);
        let used_blocks = total_data_blocks.saturating_sub(sb.free_blocks);
        
        StorageInfo {
            total_bytes: total_data_blocks * BLOCK_SIZE as u64,
            used_bytes: used_blocks * BLOCK_SIZE as u64,
            free_bytes: sb.free_blocks * BLOCK_SIZE as u64,
            total_inodes: sb.total_inodes,
            used_inodes: sb.total_inodes - sb.free_inodes,
            free_inodes: sb.free_inodes,
        }
    }
}

impl FileSystem for CottonFS {
    fn name(&self) -> &'static str {
        "cottonfs"
    }
    
    fn root(&self) -> Result<Arc<dyn Inode>, &'static str> {
        Ok(self.root.clone())
    }
    
    fn sync(&self) -> Result<(), &'static str> {
        crate::kprintln!("[CottonFS] Syncing filesystem...");
        
        // Sync all dirty inodes
        let cache = self.inode_cache.read();
        for inode in cache.values() {
            if inode.dirty.load(Ordering::Relaxed) != 0 {
                inode.sync()?;
            }
        }
        
        // Sync metadata
        self.sync_superblock()?;
        self.sync_inode_bitmap()?;
        self.sync_data_bitmap()?;
        
        crate::kprintln!("[CottonFS] Sync complete");
        Ok(())
    }
    
    fn statfs(&self) -> Result<FsStats, &'static str> {
        Ok(self.get_stats())
    }
}

// ============================================================================
// Storage Info (for About window)
// ============================================================================

/// Storage usage information
#[derive(Clone, Debug)]
pub struct StorageInfo {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub total_inodes: u64,
    pub used_inodes: u64,
    pub free_inodes: u64,
}

impl StorageInfo {
    /// Get usage percentage
    pub fn usage_percent(&self) -> u64 {
        if self.total_bytes == 0 {
            return 0;
        }
        (self.used_bytes * 100) / self.total_bytes
    }
    
    /// Format total size for display
    pub fn total_display(&self) -> String {
        format_bytes(self.total_bytes)
    }
    
    /// Format used size for display
    pub fn used_display(&self) -> String {
        format_bytes(self.used_bytes)
    }
    
    /// Format free size for display
    pub fn free_display(&self) -> String {
        format_bytes(self.free_bytes)
    }
}

/// Format bytes for human-readable display
fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        alloc::format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        alloc::format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        alloc::format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        alloc::format!("{} B", bytes)
    }
}

// ============================================================================
// CottonInode - In-memory Inode
// ============================================================================

/// In-memory inode for CottonFS
pub struct CottonInode {
    ino: u64,
    fs: *const CottonFS,
    file_type: FileType,
    disk_inode: RwLock<DiskInode>,
    /// Cached directory entries (for directories)
    dir_entries: RwLock<Option<Vec<DiskDirEntry>>>,
    /// Cached file data (for files)
    file_data: RwLock<Option<Vec<u8>>>,
    /// Dirty flag
    dirty: AtomicU64,
}

// Safety: We ensure thread-safe access via RwLock
unsafe impl Send for CottonInode {}
unsafe impl Sync for CottonInode {}

impl CottonInode {
    fn new_placeholder(ino: u64) -> Self {
        Self {
            ino,
            fs: core::ptr::null(),
            file_type: FileType::Directory,
            disk_inode: RwLock::new(DiskInode::new_dir()),
            dir_entries: RwLock::new(None),
            file_data: RwLock::new(None),
            dirty: AtomicU64::new(0),
        }
    }
    
    fn fs(&self) -> &CottonFS {
        unsafe { &*self.fs }
    }
    
    fn mark_dirty(&self) {
        self.dirty.store(1, Ordering::Relaxed);
    }
    
    /// Load directory entries from disk
    fn load_dir_entries(&self) -> Result<(), &'static str> {
        if self.file_type != FileType::Directory {
            return Err("Not a directory");
        }
        
        let disk_inode = self.disk_inode.read();
        let mut entries = Vec::new();
        
        // Read directory data from blocks
        let mut data = Vec::new();
        for i in 0..DIRECT_BLOCKS {
            if disk_inode.direct[i] == 0 {
                break;
            }
            let mut buf = vec![0u8; BLOCK_SIZE];
            read_block(&self.fs().device, disk_inode.direct[i], &mut buf)?;
            data.extend_from_slice(&buf);
        }
        
        // Parse directory entries
        let entry_size = core::mem::size_of::<DiskDirEntry>();
        let num_entries = data.len() / entry_size;
        
        for i in 0..num_entries {
            let offset = i * entry_size;
            if offset + entry_size > data.len() {
                break;
            }
            let entry: DiskDirEntry = unsafe {
                core::ptr::read(data[offset..].as_ptr() as *const DiskDirEntry)
            };
            if !entry.is_empty() {
                entries.push(entry);
            }
        }
        
        *self.dir_entries.write() = Some(entries);
        Ok(())
    }
    
    /// Save directory entries to disk
    fn save_dir_entries(&self) -> Result<(), &'static str> {
        if self.file_type != FileType::Directory {
            return Err("Not a directory");
        }
        
        let entries_opt = self.dir_entries.read();
        let entries = entries_opt.as_ref().ok_or("Directory not loaded")?;
        
        // Serialize entries
        let entry_size = core::mem::size_of::<DiskDirEntry>();
        let mut data = vec![0u8; entries.len() * entry_size];
        
        for (i, entry) in entries.iter().enumerate() {
            let offset = i * entry_size;
            let entry_bytes = unsafe {
                core::slice::from_raw_parts(entry as *const DiskDirEntry as *const u8, entry_size)
            };
            data[offset..offset + entry_size].copy_from_slice(entry_bytes);
        }
        
        drop(entries_opt);
        
        // Write to blocks (allocate if needed)
        let blocks_needed = (data.len() + BLOCK_SIZE - 1) / BLOCK_SIZE;
        let mut disk_inode = self.disk_inode.write();
        
        for i in 0..blocks_needed.min(DIRECT_BLOCKS) {
            if disk_inode.direct[i] == 0 {
                disk_inode.direct[i] = self.fs().alloc_block()?;
            }
            
            let offset = i * BLOCK_SIZE;
            let end = (offset + BLOCK_SIZE).min(data.len());
            let mut buf = vec![0u8; BLOCK_SIZE];
            buf[..end - offset].copy_from_slice(&data[offset..end]);
            
            write_block(&self.fs().device, disk_inode.direct[i], &buf)?;
        }
        
        disk_inode.size = data.len() as u64;
        disk_inode.blocks = blocks_needed as u64;
        
        drop(disk_inode);
        
        // Write inode to disk
        let disk_inode = self.disk_inode.read();
        self.fs().write_disk_inode(self.ino, &disk_inode)?;
        
        self.dirty.store(0, Ordering::Relaxed);
        Ok(())
    }
    
    /// Load file data from disk
    fn load_file_data(&self) -> Result<(), &'static str> {
        if self.file_type != FileType::Regular {
            return Err("Not a regular file");
        }
        
        let disk_inode = self.disk_inode.read();
        let size = disk_inode.size as usize;
        
        let mut data = Vec::with_capacity(size);
        let mut remaining = size;
        
        // Read from direct blocks
        for i in 0..DIRECT_BLOCKS {
            if remaining == 0 || disk_inode.direct[i] == 0 {
                break;
            }
            
            let mut buf = vec![0u8; BLOCK_SIZE];
            read_block(&self.fs().device, disk_inode.direct[i], &mut buf)?;
            
            let to_read = remaining.min(BLOCK_SIZE);
            data.extend_from_slice(&buf[..to_read]);
            remaining -= to_read;
        }
        
        *self.file_data.write() = Some(data);
        Ok(())
    }
    
    /// Save file data to disk
    fn save_file_data(&self) -> Result<(), &'static str> {
        if self.file_type != FileType::Regular {
            return Err("Not a regular file");
        }
        
        let data_opt = self.file_data.read();
        let data = data_opt.as_ref().ok_or("File data not loaded")?;
        
        let blocks_needed = (data.len() + BLOCK_SIZE - 1) / BLOCK_SIZE;
        
        drop(data_opt);
        
        let mut disk_inode = self.disk_inode.write();
        let data_opt = self.file_data.read();
        let data = data_opt.as_ref().ok_or("File data not loaded")?;
        
        // Allocate and write blocks
        for i in 0..blocks_needed.min(DIRECT_BLOCKS) {
            if disk_inode.direct[i] == 0 {
                disk_inode.direct[i] = self.fs().alloc_block()?;
            }
            
            let offset = i * BLOCK_SIZE;
            let end = (offset + BLOCK_SIZE).min(data.len());
            let mut buf = vec![0u8; BLOCK_SIZE];
            buf[..end - offset].copy_from_slice(&data[offset..end]);
            
            write_block(&self.fs().device, disk_inode.direct[i], &buf)?;
        }
        
        // Free extra blocks if file shrunk
        for i in blocks_needed..DIRECT_BLOCKS {
            if disk_inode.direct[i] != 0 {
                let _ = self.fs().free_block(disk_inode.direct[i]);
                disk_inode.direct[i] = 0;
            }
        }
        
        disk_inode.size = data.len() as u64;
        disk_inode.blocks = blocks_needed as u64;
        
        drop(data_opt);
        drop(disk_inode);
        
        // Write inode to disk
        let disk_inode = self.disk_inode.read();
        self.fs().write_disk_inode(self.ino, &disk_inode)?;
        
        self.dirty.store(0, Ordering::Relaxed);
        Ok(())
    }
}

impl Inode for CottonInode {
    fn ino(&self) -> u64 {
        self.ino
    }
    
    fn file_type(&self) -> FileType {
        self.file_type
    }
    
    fn stat(&self) -> Result<Stat, &'static str> {
        let disk_inode = self.disk_inode.read();
        Ok(Stat {
            dev: 1,
            ino: self.ino,
            mode: FileMode::from_bits_truncate(disk_inode.mode),
            nlink: disk_inode.nlink,
            uid: disk_inode.uid,
            gid: disk_inode.gid,
            rdev: 0,
            size: disk_inode.size,
            blksize: BLOCK_SIZE as u32,
            blocks: disk_inode.blocks,
            atime: disk_inode.atime,
            mtime: disk_inode.mtime,
            ctime: disk_inode.ctime,
            file_type: self.file_type,
        })
    }
    
    fn read(&self, offset: u64, buf: &mut [u8]) -> Result<usize, &'static str> {
        if self.file_type != FileType::Regular {
            return Err("Not a regular file");
        }
        
        // Load data if not cached
        {
            let data = self.file_data.read();
            if data.is_none() {
                drop(data);
                self.load_file_data()?;
            }
        }
        
        let data = self.file_data.read();
        let data = data.as_ref().ok_or("Failed to load file data")?;
        
        let offset = offset as usize;
        if offset >= data.len() {
            return Ok(0);
        }
        
        let available = data.len() - offset;
        let to_read = buf.len().min(available);
        buf[..to_read].copy_from_slice(&data[offset..offset + to_read]);
        
        Ok(to_read)
    }
    
    fn write(&self, offset: u64, buf: &[u8]) -> Result<usize, &'static str> {
        if self.file_type != FileType::Regular {
            return Err("Not a regular file");
        }
        
        // Load data if not cached
        {
            let data = self.file_data.read();
            if data.is_none() {
                drop(data);
                let _ = self.load_file_data(); // Ignore error for new files
            }
        }
        
        {
            let mut data_guard = self.file_data.write();
            let data = data_guard.get_or_insert_with(Vec::new);
            
            let offset = offset as usize;
            
            // Extend file if needed
            if offset + buf.len() > data.len() {
                data.resize(offset + buf.len(), 0);
            }
            
            data[offset..offset + buf.len()].copy_from_slice(buf);
        }
        
        self.mark_dirty();
        
        // Sync immediately for persistence
        self.save_file_data()?;
        
        Ok(buf.len())
    }
    
    fn readdir(&self) -> Result<Vec<DirEntry>, &'static str> {
        if self.file_type != FileType::Directory {
            return Err("Not a directory");
        }
        
        // Load entries if not cached
        {
            let entries = self.dir_entries.read();
            if entries.is_none() {
                drop(entries);
                self.load_dir_entries()?;
            }
        }
        
        let entries_guard = self.dir_entries.read();
        let entries = entries_guard.as_ref().ok_or("Failed to load directory")?;
        
        let mut result = Vec::new();
        
        // Add . and ..
        result.push(DirEntry {
            name: String::from("."),
            file_type: FileType::Directory,
            inode: self.ino,
        });
        
        result.push(DirEntry {
            name: String::from(".."),
            file_type: FileType::Directory,
            inode: self.ino, // TODO: track parent
        });
        
        // Add actual entries
        for entry in entries.iter() {
            result.push(DirEntry {
                name: entry.get_name(),
                file_type: match entry.file_type {
                    1 => FileType::Regular,
                    2 => FileType::Directory,
                    3 => FileType::Symlink,
                    _ => FileType::Regular,
                },
                inode: entry.inode,
            });
        }
        
        Ok(result)
    }
    
    fn lookup(&self, name: &str) -> Result<Option<Arc<dyn Inode>>, &'static str> {
        if self.file_type != FileType::Directory {
            return Err("Not a directory");
        }
        
        if name == "." {
            return Ok(None); // Handled at VFS level
        }
        
        if name == ".." {
            return Ok(None); // Handled at VFS level
        }
        
        // Load entries if not cached
        {
            let entries = self.dir_entries.read();
            if entries.is_none() {
                drop(entries);
                self.load_dir_entries()?;
            }
        }
        
        // Find the inode number first
        let target_ino = {
            let entries_guard = self.dir_entries.read();
            let entries = entries_guard.as_ref().ok_or("Failed to load directory")?;
            
            let mut found_ino = None;
            for entry in entries.iter() {
                if entry.get_name() == name {
                    found_ino = Some(entry.inode);
                    break;
                }
            }
            found_ino
        };
        
        // Load and return the inode if found
        if let Some(ino) = target_ino {
            let inode = self.fs().load_inode(ino)?;
            return Ok(Some(inode as Arc<dyn Inode>));
        }
        
        Ok(None)
    }
    
    fn create(&self, name: &str) -> Result<Arc<dyn Inode>, &'static str> {
        if self.file_type != FileType::Directory {
            return Err("Not a directory");
        }
        
        if name.len() > MAX_FILENAME {
            return Err("Filename too long");
        }
        
        // Load entries if not cached
        {
            let entries = self.dir_entries.read();
            if entries.is_none() {
                drop(entries);
                let _ = self.load_dir_entries();
            }
        }
        
        // Check if file already exists
        {
            let entries_guard = self.dir_entries.read();
            if let Some(entries) = entries_guard.as_ref() {
                for entry in entries {
                    if entry.get_name() == name {
                        return Err("File exists");
                    }
                }
            }
        }
        
        // Allocate new inode
        let ino = self.fs().alloc_inode()?;
        
        // Create disk inode
        let disk_inode = DiskInode::new_file();
        self.fs().write_disk_inode(ino, &disk_inode)?;
        
        // Add to directory
        {
            let mut entries_guard = self.dir_entries.write();
            let entries = entries_guard.get_or_insert_with(Vec::new);
            entries.push(DiskDirEntry::new(ino, name, FileType::Regular));
        }
        
        self.mark_dirty();
        self.save_dir_entries()?;
        
        // Return the new inode
        let inode = self.fs().load_inode(ino)?;
        Ok(inode as Arc<dyn Inode>)
    }
    
    fn mkdir(&self, name: &str) -> Result<Arc<dyn Inode>, &'static str> {
        if self.file_type != FileType::Directory {
            return Err("Not a directory");
        }
        
        if name.len() > MAX_FILENAME {
            return Err("Filename too long");
        }
        
        // Load entries if not cached
        {
            let entries = self.dir_entries.read();
            if entries.is_none() {
                drop(entries);
                let _ = self.load_dir_entries();
            }
        }
        
        // Check if directory already exists
        {
            let entries_guard = self.dir_entries.read();
            if let Some(entries) = entries_guard.as_ref() {
                for entry in entries {
                    if entry.get_name() == name {
                        return Err("Directory exists");
                    }
                }
            }
        }
        
        // Allocate new inode
        let ino = self.fs().alloc_inode()?;
        
        // Create disk inode
        let disk_inode = DiskInode::new_dir();
        self.fs().write_disk_inode(ino, &disk_inode)?;
        
        // Add to directory
        {
            let mut entries_guard = self.dir_entries.write();
            let entries = entries_guard.get_or_insert_with(Vec::new);
            entries.push(DiskDirEntry::new(ino, name, FileType::Directory));
        }
        
        self.mark_dirty();
        self.save_dir_entries()?;
        
        // Return the new inode
        let inode = self.fs().load_inode(ino)?;
        Ok(inode as Arc<dyn Inode>)
    }
    
    fn unlink(&self, name: &str) -> Result<(), &'static str> {
        if self.file_type != FileType::Directory {
            return Err("Not a directory");
        }
        
        // Load entries if not cached
        {
            let entries = self.dir_entries.read();
            if entries.is_none() {
                drop(entries);
                self.load_dir_entries()?;
            }
        }
        
        let inode_to_free;
        
        // Remove from directory
        {
            let mut entries_guard = self.dir_entries.write();
            let entries = entries_guard.as_mut().ok_or("Failed to load directory")?;
            
            if let Some(pos) = entries.iter().position(|e| e.get_name() == name) {
                inode_to_free = entries[pos].inode;
                entries.remove(pos);
            } else {
                return Err("File not found");
            }
        }
        
        self.mark_dirty();
        self.save_dir_entries()?;
        
        // Free the inode
        self.fs().free_inode(inode_to_free)?;
        
        Ok(())
    }
    
    fn truncate(&self, size: u64) -> Result<(), &'static str> {
        if self.file_type != FileType::Regular {
            return Err("Not a regular file");
        }
        
        // Load data if not cached
        {
            let data = self.file_data.read();
            if data.is_none() {
                drop(data);
                let _ = self.load_file_data();
            }
        }
        
        {
            let mut data_guard = self.file_data.write();
            let data = data_guard.get_or_insert_with(Vec::new);
            data.resize(size as usize, 0);
        }
        
        self.mark_dirty();
        self.save_file_data()?;
        
        Ok(())
    }
    
    fn sync(&self) -> Result<(), &'static str> {
        if self.dirty.load(Ordering::Relaxed) == 0 {
            return Ok(());
        }
        
        match self.file_type {
            FileType::Regular => self.save_file_data()?,
            FileType::Directory => self.save_dir_entries()?,
            _ => {}
        }
        
        Ok(())
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Read a block from disk
fn read_block(device: &Arc<dyn BlockDevice>, block: u64, buf: &mut [u8]) -> Result<(), &'static str> {
    let sector = block * SECTORS_PER_BLOCK;
    device.read(sector, SECTORS_PER_BLOCK as usize, buf)
}

/// Write a block to disk
fn write_block(device: &Arc<dyn BlockDevice>, block: u64, buf: &[u8]) -> Result<(), &'static str> {
    let sector = block * SECTORS_PER_BLOCK;
    // Debug: uncomment to trace disk writes
    // crate::serial_println!("[FS] write_block {} (sector {})", block, sector);
    device.write(sector, SECTORS_PER_BLOCK as usize, buf)
}

/// Get bit from bitmap
fn get_bit(bitmap: &[u8], index: usize) -> bool {
    let byte_index = index / 8;
    let bit_index = index % 8;
    if byte_index >= bitmap.len() {
        return false;
    }
    (bitmap[byte_index] >> bit_index) & 1 == 1
}

/// Set bit in bitmap
fn set_bit(bitmap: &mut [u8], index: usize) {
    let byte_index = index / 8;
    let bit_index = index % 8;
    if byte_index < bitmap.len() {
        bitmap[byte_index] |= 1 << bit_index;
    }
}

/// Clear bit in bitmap
fn clear_bit(bitmap: &mut [u8], index: usize) {
    let byte_index = index / 8;
    let bit_index = index % 8;
    if byte_index < bitmap.len() {
        bitmap[byte_index] &= !(1 << bit_index);
    }
}

// ============================================================================
// Global Storage Info Access
// ============================================================================

/// Get current storage information (for About window, etc.)
/// Returns None if filesystem is not CottonFS or not initialized
pub fn get_storage_info() -> Option<StorageInfo> {
    // Try to get from mounted root filesystem
    let mounts = super::MOUNTS.read();
    for mount in mounts.iter() {
        if mount.path == "/" && mount.fs.name() == "cottonfs" {
            // Get stats from the filesystem
            if let Ok(stats) = mount.fs.statfs() {
                let total_bytes = stats.total_blocks * stats.block_size as u64;
                let used_blocks = stats.total_blocks.saturating_sub(stats.free_blocks);
                let used_bytes = used_blocks * stats.block_size as u64;
                let free_bytes = stats.free_blocks * stats.block_size as u64;
                
                return Some(StorageInfo {
                    total_bytes,
                    used_bytes,
                    free_bytes,
                    total_inodes: stats.total_inodes,
                    used_inodes: stats.total_inodes.saturating_sub(stats.free_inodes),
                    free_inodes: stats.free_inodes,
                });
            }
        }
    }
    None
}
