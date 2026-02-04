//! ATA/IDE Driver

use spin::Mutex;

use super::BlockDevice;

/// ATA registers (primary channel)
const ATA_PRIMARY_DATA: u16 = 0x1F0;
const ATA_PRIMARY_ERROR: u16 = 0x1F1;
const ATA_PRIMARY_SECTOR_COUNT: u16 = 0x1F2;
const ATA_PRIMARY_LBA_LO: u16 = 0x1F3;
const ATA_PRIMARY_LBA_MID: u16 = 0x1F4;
const ATA_PRIMARY_LBA_HI: u16 = 0x1F5;
const ATA_PRIMARY_DRIVE: u16 = 0x1F6;
const ATA_PRIMARY_STATUS: u16 = 0x1F7;
const ATA_PRIMARY_COMMAND: u16 = 0x1F7;
const ATA_PRIMARY_CONTROL: u16 = 0x3F6;

/// ATA commands
const ATA_CMD_READ_PIO: u8 = 0x20;
const ATA_CMD_READ_PIO_EXT: u8 = 0x24;
const ATA_CMD_WRITE_PIO: u8 = 0x30;
const ATA_CMD_WRITE_PIO_EXT: u8 = 0x34;
const ATA_CMD_CACHE_FLUSH: u8 = 0xE7;
const ATA_CMD_IDENTIFY: u8 = 0xEC;

/// ATA status bits
const ATA_SR_BSY: u8 = 0x80;
const ATA_SR_DRDY: u8 = 0x40;
const ATA_SR_DRQ: u8 = 0x08;
const ATA_SR_ERR: u8 = 0x01;

/// ATA drive selection
const ATA_MASTER: u8 = 0xA0;
const ATA_SLAVE: u8 = 0xB0;

/// Maximum devices
const MAX_ATA_DEVICES: usize = 4;

/// Model string buffer size
const MODEL_SIZE: usize = 40;
const SERIAL_SIZE: usize = 20;
const NAME_SIZE: usize = 8;

/// ATA device info
pub struct AtaDevice {
    pub channel: u8,
    pub drive: u8,
    pub name: [u8; NAME_SIZE],
    pub model: [u8; MODEL_SIZE],
    pub serial: [u8; SERIAL_SIZE],
    pub sectors: u64,
    pub lba48: bool,
    pub present: bool,
}

impl AtaDevice {
    const fn empty() -> Self {
        Self {
            channel: 0,
            drive: 0,
            name: [0; NAME_SIZE],
            model: [0; MODEL_SIZE],
            serial: [0; SERIAL_SIZE],
            sectors: 0,
            lba48: false,
            present: false,
        }
    }
    
    /// Create new ATA device
    #[cfg(target_arch = "x86_64")]
    pub fn detect(channel: u8, drive: u8) -> Option<Self> {
        use crate::arch::x86_64::{inb, inw, outb};
        
        let base = if channel == 0 { ATA_PRIMARY_DATA } else { 0x170 };
        let drive_sel = if drive == 0 { ATA_MASTER } else { ATA_SLAVE };
        
        // Select drive
        outb(base + 6, drive_sel);
        
        // Delay
        for _ in 0..15 {
            inb(base + 7);
        }
        
        // Send IDENTIFY command
        outb(base + 2, 0); // Sector count
        outb(base + 3, 0); // LBA lo
        outb(base + 4, 0); // LBA mid
        outb(base + 5, 0); // LBA hi
        outb(base + 7, ATA_CMD_IDENTIFY);
        
        // Check if drive exists
        let status = inb(base + 7);
        if status == 0 {
            return None;
        }
        
        // Wait for BSY to clear with timeout
        for _ in 0..100000 {
            let status = inb(base + 7);
            if status & ATA_SR_BSY == 0 {
                break;
            }
        }
        
        // Final check
        let status = inb(base + 7);
        if status & ATA_SR_BSY != 0 {
            return None; // Timed out
        }
        
        // Check for ATAPI
        let lba_mid = inb(base + 4);
        let lba_hi = inb(base + 5);
        if lba_mid != 0 || lba_hi != 0 {
            return None; // ATAPI device
        }
        
        // Wait for DRQ with timeout
        for _ in 0..100000 {
            let status = inb(base + 7);
            if status & ATA_SR_ERR != 0 {
                return None;
            }
            if status & ATA_SR_DRQ != 0 {
                break;
            }
        }
        
        // Check DRQ is set
        let status = inb(base + 7);
        if status & ATA_SR_DRQ == 0 {
            return None;
        }
        
        // Read identification data
        let mut data = [0u16; 256];
        for i in 0..256 {
            data[i] = inw(base);
        }
        
        let mut device = Self::empty();
        device.channel = channel;
        device.drive = drive;
        device.present = true;
        
        // Extract model string (words 27-46)
        let mut model_idx = 0;
        for i in 27..47 {
            let word = data[i];
            let hi = ((word >> 8) & 0xFF) as u8;
            let lo = (word & 0xFF) as u8;
            if model_idx < MODEL_SIZE && hi != 0 {
                device.model[model_idx] = hi;
                model_idx += 1;
            }
            if model_idx < MODEL_SIZE && lo != 0 {
                device.model[model_idx] = lo;
                model_idx += 1;
            }
        }
        
        // Extract serial string (words 10-19)
        let mut serial_idx = 0;
        for i in 10..20 {
            let word = data[i];
            let hi = ((word >> 8) & 0xFF) as u8;
            let lo = (word & 0xFF) as u8;
            if serial_idx < SERIAL_SIZE && hi != 0 && hi != b' ' {
                device.serial[serial_idx] = hi;
                serial_idx += 1;
            }
            if serial_idx < SERIAL_SIZE && lo != 0 && lo != b' ' {
                device.serial[serial_idx] = lo;
                serial_idx += 1;
            }
        }
        
        // Check for LBA48 support
        device.lba48 = data[83] & (1 << 10) != 0;
        
        // Get sector count
        device.sectors = if device.lba48 {
            (data[100] as u64)
                | ((data[101] as u64) << 16)
                | ((data[102] as u64) << 32)
                | ((data[103] as u64) << 48)
        } else {
            (data[60] as u64) | ((data[61] as u64) << 16)
        };
        
        // Set name (hda, hdb, hdc, hdd)
        let name_char = match (channel, drive) {
            (0, 0) => b"hda\0\0\0\0\0",
            (0, 1) => b"hdb\0\0\0\0\0",
            (1, 0) => b"hdc\0\0\0\0\0",
            (1, 1) => b"hdd\0\0\0\0\0",
            _ => b"hdx\0\0\0\0\0",
        };
        device.name.copy_from_slice(name_char);
        
        Some(device)
    }
    
    #[cfg(not(target_arch = "x86_64"))]
    pub fn detect(_channel: u8, _drive: u8) -> Option<Self> {
        None
    }
    
    /// Get device name as str
    pub fn name_str(&self) -> &str {
        let len = self.name.iter().position(|&c| c == 0).unwrap_or(NAME_SIZE);
        core::str::from_utf8(&self.name[..len]).unwrap_or("unknown")
    }
    
    /// Get model as str
    pub fn model_str(&self) -> &str {
        let len = self.model.iter().position(|&c| c == 0).unwrap_or(MODEL_SIZE);
        core::str::from_utf8(&self.model[..len]).unwrap_or("unknown")
    }
    
    /// Read sectors using PIO
    #[cfg(target_arch = "x86_64")]
    pub fn read_sectors(&self, lba: u64, count: u8, buf: &mut [u8]) -> Result<(), &'static str> {
        use crate::arch::x86_64::{inb, inw, outb};
        
        if buf.len() < (count as usize * 512) {
            return Err("Buffer too small");
        }
        
        let base = if self.channel == 0 { ATA_PRIMARY_DATA } else { 0x170 };
        let drive_sel = if self.drive == 0 { 0xE0 } else { 0xF0 };
        
        if self.lba48 && lba > 0x0FFFFFFF {
            // LBA48 mode
            outb(base + 6, drive_sel);
            outb(base + 2, 0); // Sector count high
            outb(base + 3, ((lba >> 24) & 0xFF) as u8);
            outb(base + 4, ((lba >> 32) & 0xFF) as u8);
            outb(base + 5, ((lba >> 40) & 0xFF) as u8);
            outb(base + 2, count);
            outb(base + 3, (lba & 0xFF) as u8);
            outb(base + 4, ((lba >> 8) & 0xFF) as u8);
            outb(base + 5, ((lba >> 16) & 0xFF) as u8);
            outb(base + 7, ATA_CMD_READ_PIO_EXT);
        } else {
            // LBA28 mode
            outb(base + 6, drive_sel | ((lba >> 24) & 0x0F) as u8);
            outb(base + 2, count);
            outb(base + 3, (lba & 0xFF) as u8);
            outb(base + 4, ((lba >> 8) & 0xFF) as u8);
            outb(base + 5, ((lba >> 16) & 0xFF) as u8);
            outb(base + 7, ATA_CMD_READ_PIO);
        }
        
        for sector in 0..count as usize {
            // Wait for DRQ with timeout
            let mut ready = false;
            for _ in 0..100000 {
                let status = inb(base + 7);
                if status & ATA_SR_ERR != 0 {
                    return Err("Read error");
                }
                if status & ATA_SR_DRQ != 0 {
                    ready = true;
                    break;
                }
            }
            if !ready {
                return Err("Read timeout");
            }
            
            // Read sector
            let offset = sector * 512;
            for i in 0..256 {
                let word = inw(base);
                buf[offset + i * 2] = (word & 0xFF) as u8;
                buf[offset + i * 2 + 1] = ((word >> 8) & 0xFF) as u8;
            }
        }
        
        Ok(())
    }
    
    #[cfg(not(target_arch = "x86_64"))]
    pub fn read_sectors(&self, _lba: u64, _count: u8, _buf: &mut [u8]) -> Result<(), &'static str> {
        Err("Not supported on this platform")
    }
    
    /// Write sectors using PIO
    #[cfg(target_arch = "x86_64")]
    pub fn write_sectors(&self, lba: u64, count: u8, buf: &[u8]) -> Result<(), &'static str> {
        use crate::arch::x86_64::{inb, outb, outw};
        
        if buf.len() < (count as usize * 512) {
            return Err("Buffer too small");
        }
        
        let base = if self.channel == 0 { ATA_PRIMARY_DATA } else { 0x170 };
        let drive_sel = if self.drive == 0 { 0xE0 } else { 0xF0 };
        
        if self.lba48 && lba > 0x0FFFFFFF {
            // LBA48 mode
            outb(base + 6, drive_sel);
            outb(base + 2, 0);
            outb(base + 3, ((lba >> 24) & 0xFF) as u8);
            outb(base + 4, ((lba >> 32) & 0xFF) as u8);
            outb(base + 5, ((lba >> 40) & 0xFF) as u8);
            outb(base + 2, count);
            outb(base + 3, (lba & 0xFF) as u8);
            outb(base + 4, ((lba >> 8) & 0xFF) as u8);
            outb(base + 5, ((lba >> 16) & 0xFF) as u8);
            outb(base + 7, ATA_CMD_WRITE_PIO_EXT);
        } else {
            // LBA28 mode
            outb(base + 6, drive_sel | ((lba >> 24) & 0x0F) as u8);
            outb(base + 2, count);
            outb(base + 3, (lba & 0xFF) as u8);
            outb(base + 4, ((lba >> 8) & 0xFF) as u8);
            outb(base + 5, ((lba >> 16) & 0xFF) as u8);
            outb(base + 7, ATA_CMD_WRITE_PIO);
        }
        
        for sector in 0..count as usize {
            // Wait for DRQ with timeout
            let mut ready = false;
            for _ in 0..100000 {
                let status = inb(base + 7);
                if status & ATA_SR_ERR != 0 {
                    return Err("Write error");
                }
                if status & ATA_SR_DRQ != 0 {
                    ready = true;
                    break;
                }
            }
            if !ready {
                return Err("Write timeout");
            }
            
            // Write sector
            let offset = sector * 512;
            for i in 0..256 {
                let word = (buf[offset + i * 2] as u16) 
                         | ((buf[offset + i * 2 + 1] as u16) << 8);
                outw(base, word);
            }
        }
        
        // Flush cache
        self.flush()?;
        
        Ok(())
    }
    
    #[cfg(not(target_arch = "x86_64"))]
    pub fn write_sectors(&self, _lba: u64, _count: u8, _buf: &[u8]) -> Result<(), &'static str> {
        Err("Not supported on this platform")
    }
    
    /// Flush cache
    pub fn flush(&self) -> Result<(), &'static str> {
        #[cfg(target_arch = "x86_64")]
        {
            use crate::arch::x86_64::{inb, outb};
            let base = if self.channel == 0 { ATA_PRIMARY_DATA } else { 0x170 };
            outb(base + 7, ATA_CMD_CACHE_FLUSH);
            // Wait with timeout
            for _ in 0..100000 {
                let status = inb(base + 7);
                if status & ATA_SR_BSY == 0 {
                    break;
                }
            }
        }
        Ok(())
    }
}

impl BlockDevice for AtaDevice {
    fn name(&self) -> &str {
        self.name_str()
    }
    
    fn read(&self, start: u64, count: usize, buf: &mut [u8]) -> Result<(), &'static str> {
        if count > 255 {
            return Err("Count too large");
        }
        self.read_sectors(start, count as u8, buf)
    }
    
    fn write(&self, start: u64, count: usize, buf: &[u8]) -> Result<(), &'static str> {
        if count > 255 {
            return Err("Count too large");
        }
        self.write_sectors(start, count as u8, buf)
    }
    
    fn block_size(&self) -> usize {
        512
    }
    
    fn total_blocks(&self) -> u64 {
        self.sectors
    }
}

/// Global ATA devices
static ATA_DEVICES: Mutex<[AtaDevice; MAX_ATA_DEVICES]> = Mutex::new([
    AtaDevice::empty(),
    AtaDevice::empty(),
    AtaDevice::empty(),
    AtaDevice::empty(),
]);

/// Scan for ATA devices
pub fn scan_devices() {
    let mut devices = ATA_DEVICES.lock();
    let mut idx = 0;
    
    for channel in 0..2u8 {
        for drive in 0..2u8 {
            if let Some(device) = AtaDevice::detect(channel, drive) {
                crate::kprintln!("[ATA] Found device: {} - {} ({} sectors)",
                    device.name_str(), device.model_str(), device.sectors);
                if idx < MAX_ATA_DEVICES {
                    devices[idx] = device;
                    idx += 1;
                }
            }
        }
    }
}

/// Get ATA device by index
pub fn get_device(index: usize) -> Option<AtaDevice> {
    let devices = ATA_DEVICES.lock();
    if index < MAX_ATA_DEVICES && devices[index].present {
        // Copy the device data
        let d = &devices[index];
        Some(AtaDevice {
            channel: d.channel,
            drive: d.drive,
            name: d.name,
            model: d.model,
            serial: d.serial,
            sectors: d.sectors,
            lba48: d.lba48,
            present: d.present,
        })
    } else {
        None
    }
}

/// Initialize ATA driver
pub fn init() {
    scan_devices();
}
