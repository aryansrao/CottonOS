use alloc::format;
use alloc::string::String;
use alloc::vec;

use embedded_io::ErrorType;
use embedded_io::{Read, Write};
use embedded_tls::blocking::{Aes128GcmSha256, TlsConfig, TlsConnection, TlsContext, UnsecureProvider};
use rand_core::{CryptoRng, Error as RandError, RngCore};
use core::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy)]
enum NetIoError {
    Disconnected,
    Timeout,
    Network,
}

impl core::fmt::Display for NetIoError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Disconnected => write!(f, "disconnected"),
            Self::Timeout => write!(f, "timeout"),
            Self::Network => write!(f, "network error"),
        }
    }
}

impl core::error::Error for NetIoError {}

impl embedded_io::Error for NetIoError {
    fn kind(&self) -> embedded_io::ErrorKind {
        match self {
            Self::Disconnected => embedded_io::ErrorKind::ConnectionAborted,
            Self::Timeout => embedded_io::ErrorKind::TimedOut,
            Self::Network => embedded_io::ErrorKind::Other,
        }
    }
}

struct KernelTcpStream;

impl KernelTcpStream {
    fn connect(dst_ip: [u8; 4], dst_port: u16) -> Result<Self, NetIoError> {
        crate::drivers::network::tcp_connect(dst_ip, dst_port).map_err(|_| NetIoError::Network)?;

        let start = crate::proc::scheduler::ticks();
        while !crate::drivers::network::tcp_is_connected() && (crate::proc::scheduler::ticks() - start) < 2500 {
            crate::drivers::network::poll();
            crate::arch::halt();
        }

        if !crate::drivers::network::tcp_is_connected() {
            let _ = crate::drivers::network::tcp_close();
            return Err(NetIoError::Timeout);
        }

        Ok(Self)
    }

    fn shutdown(&mut self) {
        let _ = crate::drivers::network::tcp_close();
    }
}

impl ErrorType for KernelTcpStream {
    type Error = NetIoError;
}

impl Read for KernelTcpStream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        if buf.is_empty() {
            return Ok(0);
        }

        let start = crate::proc::scheduler::ticks();
        loop {
            crate::drivers::network::poll();

            let got = crate::drivers::network::tcp_read_into(buf);
            if got > 0 {
                return Ok(got);
            }

            if !crate::drivers::network::tcp_is_connected() {
                return Ok(0);
            }

            if (crate::proc::scheduler::ticks() - start) > 4000 {
                return Err(NetIoError::Timeout);
            }

            crate::arch::halt();
        }
    }
}

impl Write for KernelTcpStream {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        if buf.is_empty() {
            return Ok(0);
        }

        if !crate::drivers::network::tcp_is_connected() {
            return Err(NetIoError::Disconnected);
        }

        let mut sent = 0usize;
        for chunk in buf.chunks(1200) {
            crate::drivers::network::tcp_send(chunk).map_err(|_| NetIoError::Network)?;
            sent += chunk.len();
        }
        Ok(sent)
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

struct KernelRng {
    state: u64,
}

#[cfg(target_arch = "x86_64")]
static RDRAND_SUPPORT: AtomicU8 = AtomicU8::new(0);

impl KernelRng {
    fn new() -> Self {
        let (rx, tx, rx_err, tx_err, _, _) = crate::drivers::network::stats();
        let ticks = crate::proc::scheduler::ticks();
        let mut seed = ticks ^ (rx << 13) ^ (tx << 7) ^ (rx_err << 3) ^ tx_err;
        if let Some(v) = rdrand_u64() {
            seed ^= v;
        }
        if seed == 0 {
            seed = 0x9e37_79b9_7f4a_7c15;
        }
        Self { state: seed }
    }

    fn next_word(&mut self) -> u64 {
        if let Some(v) = rdrand_u64() {
            self.state ^= v.rotate_left(17);
            return v;
        }

        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }
}

#[cfg(target_arch = "x86_64")]
fn rdrand_u64() -> Option<u64> {
    if !cpu_has_rdrand() {
        return None;
    }

    let mut value: u64;
    let mut carry: u8;
    unsafe {
        core::arch::asm!(
            "rdrand {val}",
            "setc {ok}",
            val = out(reg) value,
            ok = out(reg_byte) carry,
            options(nomem, nostack)
        );
    }
    if carry != 0 {
        Some(value)
    } else {
        None
    }
}

#[cfg(target_arch = "x86_64")]
fn cpu_has_rdrand() -> bool {
    match RDRAND_SUPPORT.load(Ordering::Relaxed) {
        1 => true,
        2 => false,
        _ => {
            let has = unsafe {
                let leaf1 = core::arch::x86_64::__cpuid(1);
                (leaf1.ecx & (1 << 30)) != 0
            };
            RDRAND_SUPPORT.store(if has { 1 } else { 2 }, Ordering::Relaxed);
            has
        }
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn rdrand_u64() -> Option<u64> {
    None
}

impl RngCore for KernelRng {
    fn next_u32(&mut self) -> u32 {
        self.next_word() as u32
    }

    fn next_u64(&mut self) -> u64 {
        self.next_word()
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        let mut off = 0usize;
        while off < dest.len() {
            let word = self.next_word().to_le_bytes();
            let take = core::cmp::min(8, dest.len() - off);
            dest[off..off + take].copy_from_slice(&word[..take]);
            off += take;
        }
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), RandError> {
        self.fill_bytes(dest);
        Ok(())
    }
}

impl CryptoRng for KernelRng {}

pub fn https_get(host: &str, ip: [u8; 4], path: &str) -> Result<String, String> {
    let stream = KernelTcpStream::connect(ip, 443).map_err(|_| String::from("httpsget: tcp connect failed"))?;

    let mut read_record_buffer = vec![0u8; 16384];
    let mut write_record_buffer = vec![0u8; 4096];

    let config = TlsConfig::new()
        .with_server_name(host)
        .enable_rsa_signatures();

    let mut tls = TlsConnection::new(
        stream,
        read_record_buffer.as_mut_slice(),
        write_record_buffer.as_mut_slice(),
    );

    tls.open(TlsContext::new(
        &config,
        UnsecureProvider::new::<Aes128GcmSha256>(KernelRng::new()),
    ))
    .map_err(|e| format!("httpsget: tls handshake failed: {:?}", e))?;

    let req = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: CottonOS\r\nConnection: close\r\nAccept: */*\r\n\r\n",
        path,
        host
    );

    let req_bytes = req.as_bytes();
    let mut sent = 0usize;
    while sent < req_bytes.len() {
        let n = tls
            .write(&req_bytes[sent..])
            .map_err(|e| format!("httpsget: tls write failed: {:?}", e))?;
        if n == 0 {
            return Err(String::from("httpsget: tls write returned 0"));
        }
        sent += n;
    }
    tls.flush()
        .map_err(|e| format!("httpsget: tls flush failed: {:?}", e))?;

    let mut out = String::new();
    let mut buf = [0u8; 2048];
    loop {
        match tls.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => out.push_str(&String::from_utf8_lossy(&buf[..n])),
            Err(e) => {
                if out.is_empty() {
                    return Err(format!("httpsget: tls read failed: {:?}", e));
                }
                break;
            }
        }
    }

    match tls.close() {
        Ok(mut sock) => sock.shutdown(),
        Err((mut sock, _)) => sock.shutdown(),
    }

    if out.is_empty() {
        Err(String::from("httpsget: empty response"))
    } else {
        Ok(out)
    }
}
