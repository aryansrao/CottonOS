//! CPU identification and features for x86_64

use crate::arch::x86_64::cpuid;

/// CPU features detected via CPUID
pub struct CpuFeatures {
    pub vendor: [u8; 12],
    pub brand: [u8; 48],
    pub family: u8,
    pub model: u8,
    pub stepping: u8,
    pub has_sse: bool,
    pub has_sse2: bool,
    pub has_sse3: bool,
    pub has_ssse3: bool,
    pub has_sse4_1: bool,
    pub has_sse4_2: bool,
    pub has_avx: bool,
    pub has_avx2: bool,
    pub has_apic: bool,
    pub has_x2apic: bool,
    pub has_tsc: bool,
    pub has_msr: bool,
    pub has_pae: bool,
    pub has_nx: bool,
    pub has_vmx: bool,
    pub has_svm: bool,
    pub cores: u8,
    pub threads_per_core: u8,
}

impl CpuFeatures {
    pub fn detect() -> Self {
        let mut features = Self {
            vendor: [0; 12],
            brand: [0; 48],
            family: 0,
            model: 0,
            stepping: 0,
            has_sse: false,
            has_sse2: false,
            has_sse3: false,
            has_ssse3: false,
            has_sse4_1: false,
            has_sse4_2: false,
            has_avx: false,
            has_avx2: false,
            has_apic: false,
            has_x2apic: false,
            has_tsc: false,
            has_msr: false,
            has_pae: false,
            has_nx: false,
            has_vmx: false,
            has_svm: false,
            cores: 1,
            threads_per_core: 1,
        };

        // Get vendor string
        let (_, ebx, ecx, edx) = cpuid(0);
        features.vendor[0..4].copy_from_slice(&ebx.to_le_bytes());
        features.vendor[4..8].copy_from_slice(&edx.to_le_bytes());
        features.vendor[8..12].copy_from_slice(&ecx.to_le_bytes());

        // Get feature flags
        let (eax, _, ecx, edx) = cpuid(1);
        
        features.stepping = (eax & 0xF) as u8;
        features.model = ((eax >> 4) & 0xF) as u8;
        features.family = ((eax >> 8) & 0xF) as u8;
        
        if features.family == 0xF {
            features.family += ((eax >> 20) & 0xFF) as u8;
        }
        if features.family == 0x6 || features.family == 0xF {
            features.model += (((eax >> 16) & 0xF) << 4) as u8;
        }

        features.has_sse = (edx & (1 << 25)) != 0;
        features.has_sse2 = (edx & (1 << 26)) != 0;
        features.has_apic = (edx & (1 << 9)) != 0;
        features.has_tsc = (edx & (1 << 4)) != 0;
        features.has_msr = (edx & (1 << 5)) != 0;
        features.has_pae = (edx & (1 << 6)) != 0;

        features.has_sse3 = (ecx & (1 << 0)) != 0;
        features.has_ssse3 = (ecx & (1 << 9)) != 0;
        features.has_sse4_1 = (ecx & (1 << 19)) != 0;
        features.has_sse4_2 = (ecx & (1 << 20)) != 0;
        features.has_avx = (ecx & (1 << 28)) != 0;
        features.has_x2apic = (ecx & (1 << 21)) != 0;
        features.has_vmx = (ecx & (1 << 5)) != 0;

        // Extended features
        let (eax, _, _, _) = cpuid(0x80000000);
        if eax >= 0x80000001 {
            let (_, _, _, edx) = cpuid(0x80000001);
            features.has_nx = (edx & (1 << 20)) != 0;
        }

        // Get brand string
        if eax >= 0x80000004 {
            for i in 0..3 {
                let (eax, ebx, ecx, edx) = cpuid(0x80000002 + i);
                let offset = (i * 16) as usize;
                features.brand[offset..offset+4].copy_from_slice(&eax.to_le_bytes());
                features.brand[offset+4..offset+8].copy_from_slice(&ebx.to_le_bytes());
                features.brand[offset+8..offset+12].copy_from_slice(&ecx.to_le_bytes());
                features.brand[offset+12..offset+16].copy_from_slice(&edx.to_le_bytes());
            }
        }

        // Check for AVX2
        let (_, ebx, _, _) = cpuid(7);
        features.has_avx2 = (ebx & (1 << 5)) != 0;

        // Check for AMD SVM
        let (_, _, ecx, _) = cpuid(0x80000001);
        features.has_svm = (ecx & (1 << 2)) != 0;

        // Get core count
        let (_, ebx, _, _) = cpuid(1);
        features.threads_per_core = ((ebx >> 16) & 0xFF) as u8;
        if features.threads_per_core == 0 {
            features.threads_per_core = 1;
        }

        features
    }

    pub fn vendor_string(&self) -> &str {
        core::str::from_utf8(&self.vendor).unwrap_or("Unknown")
    }

    pub fn brand_string(&self) -> &str {
        let end = self.brand.iter().position(|&c| c == 0).unwrap_or(48);
        core::str::from_utf8(&self.brand[..end])
            .unwrap_or("Unknown")
            .trim()
    }
}

/// Read Time Stamp Counter
#[inline]
pub fn rdtsc() -> u64 {
    let low: u32;
    let high: u32;
    unsafe {
        core::arch::asm!(
            "rdtsc",
            out("eax") low,
            out("edx") high,
            options(nomem, nostack)
        );
    }
    ((high as u64) << 32) | (low as u64)
}

/// Memory barrier
#[inline]
pub fn mfence() {
    unsafe {
        core::arch::asm!("mfence", options(nomem, nostack));
    }
}

/// Store fence
#[inline]
pub fn sfence() {
    unsafe {
        core::arch::asm!("sfence", options(nomem, nostack));
    }
}

/// Load fence
#[inline]
pub fn lfence() {
    unsafe {
        core::arch::asm!("lfence", options(nomem, nostack));
    }
}
