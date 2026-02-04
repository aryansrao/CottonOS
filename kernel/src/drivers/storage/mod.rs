//! Storage Drivers
//!
//! ATA/IDE and AHCI storage drivers

pub mod ata;

use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

/// Block device trait
pub trait BlockDevice: Send + Sync {
    /// Get device name
    fn name(&self) -> &str;
    
    /// Get block size
    fn block_size(&self) -> usize;
    
    /// Get total blocks
    fn total_blocks(&self) -> u64;
    
    /// Read blocks
    fn read(&self, start: u64, count: usize, buf: &mut [u8]) -> Result<(), &'static str>;
    
    /// Write blocks
    fn write(&self, start: u64, count: usize, buf: &[u8]) -> Result<(), &'static str>;
    
    /// Flush buffers
    fn flush(&self) -> Result<(), &'static str> {
        Ok(())
    }
}

/// Registered block devices (as Arc for sharing)
static BLOCK_DEVICES: Mutex<Vec<Arc<dyn BlockDevice>>> = Mutex::new(Vec::new());

/// Register a block device
pub fn register_device(device: Arc<dyn BlockDevice>) {
    crate::kprintln!("[STORAGE] Registered device: {} ({} blocks of {} bytes)",
        device.name(),
        device.total_blocks(),
        device.block_size()
    );
    BLOCK_DEVICES.lock().push(device);
}

/// Get block device by index (returns Arc for sharing)
pub fn get_device(index: usize) -> Option<Arc<dyn BlockDevice>> {
    let devices = BLOCK_DEVICES.lock();
    devices.get(index).cloned()
}

/// Get device count
pub fn device_count() -> usize {
    BLOCK_DEVICES.lock().len()
}

/// Check if any disk is available
pub fn is_disk_available() -> bool {
    !BLOCK_DEVICES.lock().is_empty()
}

/// Initialize storage subsystem
pub fn init() {
    crate::kprintln!("[STORAGE] Initializing storage subsystem...");
    
    // Initialize ATA driver
    ata::init();
    
    // Register all detected ATA devices
    for i in 0..4 {
        if let Some(device) = ata::get_device(i) {
            register_device(Arc::new(device));
        }
    }
    
    let count = device_count();
    if count > 0 {
        crate::kprintln!("[STORAGE] Found {} block device(s)", count);
    } else {
        crate::kprintln!("[STORAGE] No block devices found - filesystem will be RAM-only");
    }
}

/// Partition table entry
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct MbrPartition {
    pub status: u8,
    pub first_chs: [u8; 3],
    pub part_type: u8,
    pub last_chs: [u8; 3],
    pub first_lba: u32,
    pub sector_count: u32,
}

impl MbrPartition {
    pub fn is_active(&self) -> bool {
        self.status == 0x80
    }
    
    pub fn is_valid(&self) -> bool {
        self.part_type != 0 && self.sector_count > 0
    }
}

/// Read MBR partition table
pub fn read_mbr(device: &dyn BlockDevice) -> Result<[MbrPartition; 4], &'static str> {
    let mut buf = [0u8; 512];
    device.read(0, 1, &mut buf)?;
    
    // Check MBR signature
    if buf[510] != 0x55 || buf[511] != 0xAA {
        return Err("Invalid MBR signature");
    }
    
    // Read partition entries
    let mut partitions = [MbrPartition {
        status: 0,
        first_chs: [0; 3],
        part_type: 0,
        last_chs: [0; 3],
        first_lba: 0,
        sector_count: 0,
    }; 4];
    
    for i in 0..4 {
        let offset = 446 + i * 16;
        partitions[i] = unsafe {
            core::ptr::read_unaligned(buf.as_ptr().add(offset) as *const MbrPartition)
        };
    }
    
    Ok(partitions)
}

/// GPT header
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct GptHeader {
    pub signature: [u8; 8],
    pub revision: u32,
    pub header_size: u32,
    pub header_crc32: u32,
    pub reserved: u32,
    pub current_lba: u64,
    pub backup_lba: u64,
    pub first_usable_lba: u64,
    pub last_usable_lba: u64,
    pub disk_guid: [u8; 16],
    pub partition_entry_lba: u64,
    pub num_partition_entries: u32,
    pub partition_entry_size: u32,
    pub partition_entry_crc32: u32,
}

impl GptHeader {
    pub fn is_valid(&self) -> bool {
        &self.signature == b"EFI PART"
    }
}

/// GPT partition entry
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct GptPartition {
    pub type_guid: [u8; 16],
    pub unique_guid: [u8; 16],
    pub first_lba: u64,
    pub last_lba: u64,
    pub attributes: u64,
    pub name: [u16; 36],
}

impl GptPartition {
    pub fn is_valid(&self) -> bool {
        self.type_guid != [0u8; 16]
    }
}

/// Read GPT partition table
pub fn read_gpt(device: &dyn BlockDevice) -> Result<Vec<GptPartition>, &'static str> {
    // Read GPT header (LBA 1)
    let mut buf = [0u8; 512];
    device.read(1, 1, &mut buf)?;
    
    let header: GptHeader = unsafe {
        core::ptr::read_unaligned(buf.as_ptr() as *const GptHeader)
    };
    
    if !header.is_valid() {
        return Err("Invalid GPT signature");
    }
    
    // Read partition entries
    let entries_per_sector = 512 / header.partition_entry_size as usize;
    let sectors_needed = (header.num_partition_entries as usize + entries_per_sector - 1) / entries_per_sector;
    
    let mut partitions = Vec::new();
    let mut entry_buf = [0u8; 512];
    
    for sector in 0..sectors_needed {
        device.read(header.partition_entry_lba + sector as u64, 1, &mut entry_buf)?;
        
        for i in 0..entries_per_sector {
            if partitions.len() >= header.num_partition_entries as usize {
                break;
            }
            
            let offset = i * header.partition_entry_size as usize;
            let entry: GptPartition = unsafe {
                core::ptr::read_unaligned(entry_buf.as_ptr().add(offset) as *const GptPartition)
            };
            
            if entry.is_valid() {
                partitions.push(entry);
            }
        }
    }
    
    Ok(partitions)
}