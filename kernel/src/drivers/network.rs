//! Network Driver Stack
//!
//! Current implementation:
//! - PCI device scan
//! - Realtek RTL8139 initialization
//! - Ethernet RX/TX
//! - ARP (reply + cache)
//! - IPv4 + ICMP echo reply

use alloc::collections::VecDeque;
use core::sync::atomic::{AtomicU16, AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

use crate::arch::x86_64::{inb, inl, inw, outb, outl, outw};

const PCI_CONFIG_ADDRESS: u16 = 0xCF8;
const PCI_CONFIG_DATA: u16 = 0xCFC;

const RTL8139_VENDOR_ID: u16 = 0x10EC;
const RTL8139_DEVICE_ID: u16 = 0x8139;

const RX_BUFFER_SIZE: usize = 8192 + 16 + 1500;
const TX_BUFFER_SIZE: usize = 2048;

const REG_IDR0: u16 = 0x00;
const REG_TSD0: u16 = 0x10;
const REG_TSAD0: u16 = 0x20;
const REG_RBSTART: u16 = 0x30;
const REG_CAPR: u16 = 0x38;
const REG_CBR: u16 = 0x3A;
const REG_IMR: u16 = 0x3C;
const REG_ISR: u16 = 0x3E;
const REG_TCR: u16 = 0x40;
const REG_RCR: u16 = 0x44;
const REG_CONFIG1: u16 = 0x52;
const REG_CMD: u16 = 0x37;

const CMD_RESET: u8 = 1 << 4;
const CMD_RX_ENABLE: u8 = 1 << 3;
const CMD_TX_ENABLE: u8 = 1 << 2;
const CMD_RX_EMPTY: u8 = 1 << 0;

const ISR_RX_OK: u16 = 1 << 0;
const ISR_RX_ERR: u16 = 1 << 1;
const ISR_TX_OK: u16 = 1 << 2;
const ISR_TX_ERR: u16 = 1 << 3;
const ISR_RX_OVERFLOW: u16 = 1 << 4;

const ETH_TYPE_ARP: u16 = 0x0806;
const ETH_TYPE_IPV4: u16 = 0x0800;

const IP_PROTO_ICMP: u8 = 1;
const IP_PROTO_TCP: u8 = 6;
const IP_PROTO_UDP: u8 = 17;

const DEFAULT_IP: [u8; 4] = [10, 0, 2, 15];
const DEFAULT_NETMASK: [u8; 4] = [255, 255, 255, 0];
const DEFAULT_GATEWAY: [u8; 4] = [10, 0, 2, 2];
const DEFAULT_DNS: [u8; 4] = [10, 0, 2, 3];
const TCP_RECV_BUF_SIZE: usize = 32 * 1024;

const BROADCAST_MAC: [u8; 6] = [0xFF; 6];

#[derive(Clone, Copy)]
struct NetConfig {
    ip: [u8; 4],
    netmask: [u8; 4],
    gateway: [u8; 4],
    dns: [u8; 4],
}

impl NetConfig {
    const fn default() -> Self {
        Self {
            ip: DEFAULT_IP,
            netmask: DEFAULT_NETMASK,
            gateway: DEFAULT_GATEWAY,
            dns: DEFAULT_DNS,
        }
    }
}

#[derive(Clone, Copy)]
struct PciLocation {
    bus: u8,
    slot: u8,
    func: u8,
}

struct Rtl8139 {
    io_base: u16,
    irq: u8,
    mac: [u8; 6],
    rx_buffer_phys: u64,
    rx_offset: usize,
    tx_buffers_phys: [u64; 4],
    tx_cur: usize,
}

#[derive(Clone)]
struct FramePacket {
    len: usize,
    data: [u8; 1600],
}

#[derive(Clone)]
struct UdpDatagram {
    src_ip: [u8; 4],
    src_port: u16,
    dst_port: u16,
    len: usize,
    payload: [u8; 1024],
}

#[derive(Clone, Copy)]
struct ArpEntry {
    ip: [u8; 4],
    mac: [u8; 6],
    valid: bool,
}

impl ArpEntry {
    const fn empty() -> Self {
        Self {
            ip: [0; 4],
            mac: [0; 6],
            valid: false,
        }
    }
}

static RTL8139: Mutex<Option<Rtl8139>> = Mutex::new(None);
static ARP_CACHE: Mutex<[ArpEntry; 8]> = Mutex::new([ArpEntry::empty(); 8]);
static NET_CONFIG: Mutex<NetConfig> = Mutex::new(NetConfig::default());
static RX_FRAME_QUEUE: Mutex<VecDeque<FramePacket>> = Mutex::new(VecDeque::new());
static UDP_RX_QUEUE: Mutex<VecDeque<UdpDatagram>> = Mutex::new(VecDeque::new());

#[derive(Clone)]
struct TcpClient {
    active: bool,
    connected: bool,
    dst_ip: [u8; 4],
    dst_port: u16,
    src_port: u16,
    seq: u32,
    ack: u32,
    recv_buf: [u8; TCP_RECV_BUF_SIZE],
    recv_len: usize,
}

impl TcpClient {
    const fn new() -> Self {
        Self {
            active: false,
            connected: false,
            dst_ip: [0; 4],
            dst_port: 0,
            src_port: 0,
            seq: 0,
            ack: 0,
            recv_buf: [0; TCP_RECV_BUF_SIZE],
            recv_len: 0,
        }
    }

    fn reset(&mut self) {
        self.active = false;
        self.connected = false;
        self.dst_ip = [0; 4];
        self.dst_port = 0;
        self.src_port = 0;
        self.seq = 0;
        self.ack = 0;
        self.recv_len = 0;
    }
}

static TCP_CLIENT: Mutex<TcpClient> = Mutex::new(TcpClient::new());

static RX_PACKETS: AtomicU64 = AtomicU64::new(0);
static TX_PACKETS: AtomicU64 = AtomicU64::new(0);
static RX_ERRORS: AtomicU64 = AtomicU64::new(0);
static TX_ERRORS: AtomicU64 = AtomicU64::new(0);
static ICMP_ECHO_RX: AtomicU64 = AtomicU64::new(0);
static ICMP_ECHO_TX: AtomicU64 = AtomicU64::new(0);
static PING_SEQ: AtomicU16 = AtomicU16::new(1);
static TCP_SRC_PORT_SEQ: AtomicU16 = AtomicU16::new(49152);
static TCP_SEQ_GEN: AtomicU32 = AtomicU32::new(0x1020_3040);
static DHCP_XID_GEN: AtomicU32 = AtomicU32::new(0x434F_5454);
static DNS_ID_GEN: AtomicU16 = AtomicU16::new(0x2200);

fn pci_read_u32(bus: u8, slot: u8, func: u8, offset: u8) -> u32 {
    let address = (1u32 << 31)
        | ((bus as u32) << 16)
        | ((slot as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC);

    outl(PCI_CONFIG_ADDRESS, address);
    inl(PCI_CONFIG_DATA)
}

fn pci_write_u16(bus: u8, slot: u8, func: u8, offset: u8, value: u16) {
    let aligned = offset & 0xFC;
    let shift = ((offset & 0x02) * 8) as u32;
    let mut current = pci_read_u32(bus, slot, func, aligned);
    current &= !(0xFFFF << shift);
    current |= (value as u32) << shift;

    let address = (1u32 << 31)
        | ((bus as u32) << 16)
        | ((slot as u32) << 11)
        | ((func as u32) << 8)
        | ((aligned as u32) & 0xFC);

    outl(PCI_CONFIG_ADDRESS, address);
    outl(PCI_CONFIG_DATA, current);
}

fn find_rtl8139() -> Option<PciLocation> {
    for bus in 0..=255 {
        for slot in 0..32 {
            let vendor_device = pci_read_u32(bus, slot, 0, 0x00);
            if vendor_device == 0xFFFF_FFFF {
                continue;
            }

            let vendor = (vendor_device & 0xFFFF) as u16;
            let device = ((vendor_device >> 16) & 0xFFFF) as u16;
            if vendor == RTL8139_VENDOR_ID && device == RTL8139_DEVICE_ID {
                return Some(PciLocation { bus, slot, func: 0 });
            }
        }
    }
    None
}

fn io_read_u8(io_base: u16, reg: u16) -> u8 {
    inb(io_base + reg)
}

fn io_read_u16(io_base: u16, reg: u16) -> u16 {
    inw(io_base + reg)
}

fn io_read_u32(io_base: u16, reg: u16) -> u32 {
    inl(io_base + reg)
}

fn io_write_u8(io_base: u16, reg: u16, value: u8) {
    outb(io_base + reg, value);
}

fn io_write_u16(io_base: u16, reg: u16, value: u16) {
    outw(io_base + reg, value);
}

fn io_write_u32(io_base: u16, reg: u16, value: u32) {
    outl(io_base + reg, value);
}

fn checksum16(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 1 < data.len() {
        let word = u16::from_be_bytes([data[i], data[i + 1]]) as u32;
        sum = sum.wrapping_add(word);
        i += 2;
    }

    if i < data.len() {
        sum = sum.wrapping_add((data[i] as u32) << 8);
    }

    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }

    !(sum as u16)
}

fn config() -> NetConfig {
    *NET_CONFIG.lock()
}

fn config_ip() -> [u8; 4] {
    NET_CONFIG.lock().ip
}

fn config_gateway() -> [u8; 4] {
    NET_CONFIG.lock().gateway
}

fn config_dns() -> [u8; 4] {
    NET_CONFIG.lock().dns
}

fn same_subnet(ip: [u8; 4], mask: [u8; 4], other: [u8; 4]) -> bool {
    (ip[0] & mask[0]) == (other[0] & mask[0])
        && (ip[1] & mask[1]) == (other[1] & mask[1])
        && (ip[2] & mask[2]) == (other[2] & mask[2])
        && (ip[3] & mask[3]) == (other[3] & mask[3])
}

fn route_next_hop(dst_ip: [u8; 4]) -> [u8; 4] {
    let cfg = config();
    if same_subnet(cfg.ip, cfg.netmask, dst_ip) {
        dst_ip
    } else {
        cfg.gateway
    }
}

fn ip_is_broadcast(ip: [u8; 4]) -> bool {
    ip == [255, 255, 255, 255]
}

fn sum16_words(mut sum: u32, data: &[u8]) -> u32 {
    let mut i = 0;
    while i + 1 < data.len() {
        sum = sum.wrapping_add(u16::from_be_bytes([data[i], data[i + 1]]) as u32);
        i += 2;
    }
    if i < data.len() {
        sum = sum.wrapping_add((data[i] as u32) << 8);
    }
    sum
}

fn finalize_checksum(mut sum: u32) -> u16 {
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

fn tcp_checksum(src_ip: [u8; 4], dst_ip: [u8; 4], tcp_segment: &[u8]) -> u16 {
    let mut sum = 0u32;
    sum = sum16_words(sum, &src_ip);
    sum = sum16_words(sum, &dst_ip);
    sum = sum.wrapping_add(IP_PROTO_TCP as u32);
    sum = sum.wrapping_add(tcp_segment.len() as u32);
    sum = sum16_words(sum, tcp_segment);
    finalize_checksum(sum)
}

fn udp_checksum(src_ip: [u8; 4], dst_ip: [u8; 4], udp_segment: &[u8]) -> u16 {
    let mut sum = 0u32;
    sum = sum16_words(sum, &src_ip);
    sum = sum16_words(sum, &dst_ip);
    sum = sum.wrapping_add(IP_PROTO_UDP as u32);
    sum = sum.wrapping_add(udp_segment.len() as u32);
    sum = sum16_words(sum, udp_segment);
    finalize_checksum(sum)
}

fn cache_arp(ip: [u8; 4], mac: [u8; 6]) {
    let mut cache = ARP_CACHE.lock();

    for entry in cache.iter_mut() {
        if entry.valid && entry.ip == ip {
            entry.mac = mac;
            return;
        }
    }

    for entry in cache.iter_mut() {
        if !entry.valid {
            entry.ip = ip;
            entry.mac = mac;
            entry.valid = true;
            return;
        }
    }

    cache[0] = ArpEntry {
        ip,
        mac,
        valid: true,
    };
}

fn lookup_arp(ip: [u8; 4]) -> Option<[u8; 6]> {
    let cache = ARP_CACHE.lock();
    cache
        .iter()
        .find(|entry| entry.valid && entry.ip == ip)
        .map(|entry| entry.mac)
}

fn resolve_arp(next_hop: [u8; 4], timeout_ticks: u64) -> Result<[u8; 6], &'static str> {
    if let Some(mac) = lookup_arp(next_hop) {
        return Ok(mac);
    }

    for _ in 0..3 {
        let _ = request_arp(next_hop);
        let start = crate::proc::scheduler::ticks();
        while (crate::proc::scheduler::ticks() - start) < timeout_ticks {
            poll();
            if let Some(mac) = lookup_arp(next_hop) {
                return Ok(mac);
            }
            crate::arch::halt();
        }
    }

    Err("no ARP entry for destination/next-hop")
}

impl Rtl8139 {
    fn init() -> Result<Self, &'static str> {
        let loc = find_rtl8139().ok_or("RTL8139 not found")?;

        let bar0 = pci_read_u32(loc.bus, loc.slot, loc.func, 0x10);
        if bar0 == 0 || (bar0 & 0x1) == 0 {
            return Err("RTL8139 BAR0 not I/O-mapped");
        }
        let io_base = (bar0 & 0xFFFC) as u16;

        let irq_line = (pci_read_u32(loc.bus, loc.slot, loc.func, 0x3C) & 0xFF) as u8;

        let command = (pci_read_u32(loc.bus, loc.slot, loc.func, 0x04) & 0xFFFF) as u16;
        let command = command | (1 << 0) | (1 << 2);
        pci_write_u16(loc.bus, loc.slot, loc.func, 0x04, command);

        io_write_u8(io_base, REG_CONFIG1, 0x00);

        io_write_u8(io_base, REG_CMD, CMD_RESET);
        for _ in 0..100_000 {
            if io_read_u8(io_base, REG_CMD) & CMD_RESET == 0 {
                break;
            }
        }
        if io_read_u8(io_base, REG_CMD) & CMD_RESET != 0 {
            return Err("RTL8139 reset timeout");
        }

        let rx_buffer_phys = crate::mm::physical::alloc_frames(3).ok_or("No memory for RX ring")?;
        let mut tx_buffers_phys = [0u64; 4];
        for entry in &mut tx_buffers_phys {
            *entry = crate::mm::physical::alloc_frame().ok_or("No memory for TX buffer")?;
        }

        io_write_u32(io_base, REG_RBSTART, rx_buffer_phys as u32);
        for (i, addr) in tx_buffers_phys.iter().enumerate() {
            io_write_u32(io_base, REG_TSAD0 + (i as u16 * 4), *addr as u32);
        }

        io_write_u16(io_base, REG_IMR, 0x0000);
        io_write_u16(io_base, REG_ISR, 0xFFFF);

        io_write_u32(io_base, REG_RCR, 0x0000_000F | (1 << 7) | (6 << 8));
        io_write_u32(io_base, REG_TCR, 0x0300_0700);

        io_write_u8(io_base, REG_CMD, CMD_RX_ENABLE | CMD_TX_ENABLE);

        let mut mac = [0u8; 6];
        for (i, byte) in mac.iter_mut().enumerate() {
            *byte = io_read_u8(io_base, REG_IDR0 + i as u16);
        }

        Ok(Self {
            io_base,
            irq: irq_line,
            mac,
            rx_buffer_phys,
            rx_offset: 0,
            tx_buffers_phys,
            tx_cur: 0,
        })
    }

    fn send_frame(&mut self, frame: &[u8]) -> Result<(), &'static str> {
        if frame.len() > TX_BUFFER_SIZE {
            return Err("Frame too large");
        }

        let tx_idx = self.tx_cur % 4;
        let tx_addr = self.tx_buffers_phys[tx_idx] as *mut u8;
        unsafe {
            core::ptr::copy_nonoverlapping(frame.as_ptr(), tx_addr, frame.len());
        }

        io_write_u32(
            self.io_base,
            REG_TSD0 + (tx_idx as u16 * 4),
            frame.len() as u32,
        );
        self.tx_cur = (self.tx_cur + 1) % 4;
        TX_PACKETS.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn send_arp_reply(
        &mut self,
        dst_mac: [u8; 6],
        sender_ip: [u8; 4],
        target_mac: [u8; 6],
        target_ip: [u8; 4],
    ) {
        let mut frame = [0u8; 42];
        frame[0..6].copy_from_slice(&dst_mac);
        frame[6..12].copy_from_slice(&self.mac);
        frame[12..14].copy_from_slice(&ETH_TYPE_ARP.to_be_bytes());

        frame[14..16].copy_from_slice(&1u16.to_be_bytes());
        frame[16..18].copy_from_slice(&ETH_TYPE_IPV4.to_be_bytes());
        frame[18] = 6;
        frame[19] = 4;
        frame[20..22].copy_from_slice(&2u16.to_be_bytes());

        frame[22..28].copy_from_slice(&target_mac);
        frame[28..32].copy_from_slice(&target_ip);
        frame[32..38].copy_from_slice(&sender_ip);
        frame[38..42].copy_from_slice(&sender_ip);

        let _ = self.send_frame(&frame);
    }

    fn send_arp_request(&mut self, target_ip: [u8; 4]) {
        let mut frame = [0u8; 42];
        frame[0..6].copy_from_slice(&BROADCAST_MAC);
        frame[6..12].copy_from_slice(&self.mac);
        frame[12..14].copy_from_slice(&ETH_TYPE_ARP.to_be_bytes());

        frame[14..16].copy_from_slice(&1u16.to_be_bytes());
        frame[16..18].copy_from_slice(&ETH_TYPE_IPV4.to_be_bytes());
        frame[18] = 6;
        frame[19] = 4;
        frame[20..22].copy_from_slice(&1u16.to_be_bytes());
        frame[22..28].copy_from_slice(&self.mac);
        frame[28..32].copy_from_slice(&config_ip());
        frame[32..38].copy_from_slice(&[0u8; 6]);
        frame[38..42].copy_from_slice(&target_ip);

        let _ = self.send_frame(&frame);
    }

    fn send_icmp_echo(&mut self, dst_ip: [u8; 4], dst_mac: [u8; 6], id: u16, seq: u16) {
        let payload: &[u8] = b"CottonOS ping";
        let icmp_len = 8 + payload.len();
        let total_ip_len = 20 + icmp_len;
        let frame_len = 14 + total_ip_len;
        let mut frame = [0u8; 128];

        frame[0..6].copy_from_slice(&dst_mac);
        frame[6..12].copy_from_slice(&self.mac);
        frame[12..14].copy_from_slice(&ETH_TYPE_IPV4.to_be_bytes());

        let ip = &mut frame[14..34];
        ip[0] = 0x45;
        ip[1] = 0;
        ip[2..4].copy_from_slice(&(total_ip_len as u16).to_be_bytes());
        ip[4..6].copy_from_slice(&0u16.to_be_bytes());
        ip[6..8].copy_from_slice(&0u16.to_be_bytes());
        ip[8] = 64;
        ip[9] = IP_PROTO_ICMP;
        ip[10..12].copy_from_slice(&0u16.to_be_bytes());
        ip[12..16].copy_from_slice(&config_ip());
        ip[16..20].copy_from_slice(&dst_ip);
        let ip_sum = checksum16(ip);
        ip[10..12].copy_from_slice(&ip_sum.to_be_bytes());

        let icmp = &mut frame[34..(34 + icmp_len)];
        icmp[0] = 8;
        icmp[1] = 0;
        icmp[2..4].copy_from_slice(&0u16.to_be_bytes());
        icmp[4..6].copy_from_slice(&id.to_be_bytes());
        icmp[6..8].copy_from_slice(&seq.to_be_bytes());
        icmp[8..(8 + payload.len())].copy_from_slice(payload);
        let icmp_sum = checksum16(icmp);
        icmp[2..4].copy_from_slice(&icmp_sum.to_be_bytes());

        let _ = self.send_frame(&frame[..frame_len]);
        ICMP_ECHO_TX.fetch_add(1, Ordering::Relaxed);
    }

    fn handle_arp(&mut self, src_mac: [u8; 6], payload: &[u8]) {
        if payload.len() < 28 {
            return;
        }

        let opcode = u16::from_be_bytes([payload[6], payload[7]]);
        let sender_mac = [
            payload[8], payload[9], payload[10], payload[11], payload[12], payload[13],
        ];
        let sender_ip = [payload[14], payload[15], payload[16], payload[17]];
        let target_ip = [payload[24], payload[25], payload[26], payload[27]];

        cache_arp(sender_ip, sender_mac);

        if opcode == 1 && target_ip == config_ip() {
            self.send_arp_reply(src_mac, sender_ip, self.mac, config_ip());
        }
    }

    fn handle_ipv4(&mut self, src_mac: [u8; 6], payload: &[u8]) {
        if payload.len() < 20 {
            return;
        }
        let ihl = ((payload[0] & 0x0F) as usize) * 4;
        if ihl < 20 || payload.len() < ihl {
            return;
        }

        let total_len = u16::from_be_bytes([payload[2], payload[3]]) as usize;
        if total_len < ihl || total_len > payload.len() {
            return;
        }

        let protocol = payload[9];
        let src_ip = [payload[12], payload[13], payload[14], payload[15]];
        let dst_ip = [payload[16], payload[17], payload[18], payload[19]];

        cache_arp(src_ip, src_mac);

        if dst_ip != config_ip() {
            if !(protocol == IP_PROTO_UDP && ip_is_broadcast(dst_ip)) {
                return;
            }
        }

        if protocol == IP_PROTO_ICMP {
            self.handle_icmp_echo(src_mac, src_ip, &payload[..total_len], ihl);
        } else if protocol == IP_PROTO_TCP {
            self.handle_tcp(src_mac, src_ip, &payload[..total_len], ihl);
        } else if protocol == IP_PROTO_UDP {
            self.handle_udp(src_mac, src_ip, &payload[..total_len], ihl);
        }
    }

    fn handle_udp(&mut self, src_mac: [u8; 6], src_ip: [u8; 4], packet: &[u8], ihl: usize) {
        if packet.len() < ihl + 8 {
            return;
        }

        cache_arp(src_ip, src_mac);

        let udp = &packet[ihl..];
        let src_port = u16::from_be_bytes([udp[0], udp[1]]);
        let dst_port = u16::from_be_bytes([udp[2], udp[3]]);
        let udp_len = u16::from_be_bytes([udp[4], udp[5]]) as usize;
        if udp_len < 8 || udp_len > udp.len() {
            return;
        }

        let payload = &udp[8..udp_len];
        let copy_len = core::cmp::min(payload.len(), 1024);
        let mut datagram = UdpDatagram {
            src_ip,
            src_port,
            dst_port,
            len: copy_len,
            payload: [0; 1024],
        };
        datagram.payload[..copy_len].copy_from_slice(&payload[..copy_len]);

        let mut queue = UDP_RX_QUEUE.lock();
        if queue.len() >= 32 {
            queue.pop_front();
        }
        queue.push_back(datagram);
    }

    fn send_udp_packet(
        &mut self,
        dst_ip: [u8; 4],
        dst_mac: [u8; 6],
        src_port: u16,
        dst_port: u16,
        payload: &[u8],
    ) -> Result<(), &'static str> {
        let udp_len = 8 + payload.len();
        let total_ip_len = 20 + udp_len;
        let frame_len = 14 + total_ip_len;
        if frame_len > 1600 {
            return Err("udp frame too large");
        }

        let src_ip = config_ip();
        let mut frame = [0u8; 1600];
        frame[0..6].copy_from_slice(&dst_mac);
        frame[6..12].copy_from_slice(&self.mac);
        frame[12..14].copy_from_slice(&ETH_TYPE_IPV4.to_be_bytes());

        let ip = &mut frame[14..34];
        ip[0] = 0x45;
        ip[1] = 0;
        ip[2..4].copy_from_slice(&(total_ip_len as u16).to_be_bytes());
        ip[4..6].copy_from_slice(&0u16.to_be_bytes());
        ip[6..8].copy_from_slice(&0x4000u16.to_be_bytes());
        ip[8] = 64;
        ip[9] = IP_PROTO_UDP;
        ip[10..12].copy_from_slice(&0u16.to_be_bytes());
        ip[12..16].copy_from_slice(&src_ip);
        ip[16..20].copy_from_slice(&dst_ip);
        let ip_sum = checksum16(ip);
        ip[10..12].copy_from_slice(&ip_sum.to_be_bytes());

        let udp = &mut frame[34..(34 + udp_len)];
        udp[0..2].copy_from_slice(&src_port.to_be_bytes());
        udp[2..4].copy_from_slice(&dst_port.to_be_bytes());
        udp[4..6].copy_from_slice(&(udp_len as u16).to_be_bytes());
        udp[6..8].copy_from_slice(&0u16.to_be_bytes());
        udp[8..(8 + payload.len())].copy_from_slice(payload);
        let sum = udp_checksum(src_ip, dst_ip, udp);
        udp[6..8].copy_from_slice(&sum.to_be_bytes());

        self.send_frame(&frame[..frame_len])
    }

    fn send_udp_broadcast_packet(
        &mut self,
        src_port: u16,
        dst_port: u16,
        payload: &[u8],
    ) -> Result<(), &'static str> {
        self.send_udp_packet([255, 255, 255, 255], BROADCAST_MAC, src_port, dst_port, payload)
    }

    fn handle_icmp_echo(&mut self, src_mac: [u8; 6], src_ip: [u8; 4], packet: &[u8], ihl: usize) {
        if packet.len() < ihl + 8 {
            return;
        }

        let icmp = &packet[ihl..];
        if icmp[0] != 8 {
            return;
        }

        let total_ip_len = packet.len();
        let mut frame = [0u8; 1600];

        frame[0..6].copy_from_slice(&src_mac);
        frame[6..12].copy_from_slice(&self.mac);
        frame[12..14].copy_from_slice(&ETH_TYPE_IPV4.to_be_bytes());

        let ip_out = &mut frame[14..34];
        ip_out[0] = 0x45;
        ip_out[1] = 0;
        ip_out[2..4].copy_from_slice(&(total_ip_len as u16).to_be_bytes());
        ip_out[4..6].copy_from_slice(&packet[4..6]);
        ip_out[6..8].copy_from_slice(&packet[6..8]);
        ip_out[8] = 64;
        ip_out[9] = IP_PROTO_ICMP;
        ip_out[10..12].copy_from_slice(&0u16.to_be_bytes());
        ip_out[12..16].copy_from_slice(&config_ip());
        ip_out[16..20].copy_from_slice(&src_ip);
        let ip_sum = checksum16(ip_out);
        ip_out[10..12].copy_from_slice(&ip_sum.to_be_bytes());

        let icmp_len = total_ip_len - ihl;
        let icmp_out = &mut frame[34..(34 + icmp_len)];
        icmp_out.copy_from_slice(&packet[ihl..]);
        icmp_out[0] = 0;
        icmp_out[2..4].copy_from_slice(&0u16.to_be_bytes());
        let icmp_sum = checksum16(icmp_out);
        icmp_out[2..4].copy_from_slice(&icmp_sum.to_be_bytes());

        let _ = self.send_frame(&frame[..(14 + total_ip_len)]);
        ICMP_ECHO_RX.fetch_add(1, Ordering::Relaxed);
    }

    fn send_tcp_segment(
        &mut self,
        dst_ip: [u8; 4],
        dst_mac: [u8; 6],
        src_port: u16,
        dst_port: u16,
        seq: u32,
        ack: u32,
        flags: u16,
        payload: &[u8],
    ) -> Result<(), &'static str> {
        let tcp_len = 20 + payload.len();
        let total_ip_len = 20 + tcp_len;
        let frame_len = 14 + total_ip_len;
        if frame_len > 1600 {
            return Err("tcp frame too large");
        }

        let mut frame = [0u8; 1600];
        frame[0..6].copy_from_slice(&dst_mac);
        frame[6..12].copy_from_slice(&self.mac);
        frame[12..14].copy_from_slice(&ETH_TYPE_IPV4.to_be_bytes());

        let src_ip = config_ip();
        let ip = &mut frame[14..34];
        ip[0] = 0x45;
        ip[1] = 0;
        ip[2..4].copy_from_slice(&(total_ip_len as u16).to_be_bytes());
        ip[4..6].copy_from_slice(&0u16.to_be_bytes());
        ip[6..8].copy_from_slice(&0x4000u16.to_be_bytes());
        ip[8] = 64;
        ip[9] = IP_PROTO_TCP;
        ip[10..12].copy_from_slice(&0u16.to_be_bytes());
        ip[12..16].copy_from_slice(&src_ip);
        ip[16..20].copy_from_slice(&dst_ip);
        let ip_sum = checksum16(ip);
        ip[10..12].copy_from_slice(&ip_sum.to_be_bytes());

        let tcp = &mut frame[34..(34 + tcp_len)];
        tcp[0..2].copy_from_slice(&src_port.to_be_bytes());
        tcp[2..4].copy_from_slice(&dst_port.to_be_bytes());
        tcp[4..8].copy_from_slice(&seq.to_be_bytes());
        tcp[8..12].copy_from_slice(&ack.to_be_bytes());
        tcp[12] = 5u8 << 4;
        tcp[13] = (flags & 0xFF) as u8;
        tcp[14..16].copy_from_slice(&64240u16.to_be_bytes());
        tcp[16..18].copy_from_slice(&0u16.to_be_bytes());
        tcp[18..20].copy_from_slice(&0u16.to_be_bytes());
        tcp[20..(20 + payload.len())].copy_from_slice(payload);
        let sum = tcp_checksum(src_ip, dst_ip, tcp);
        tcp[16..18].copy_from_slice(&sum.to_be_bytes());

        self.send_frame(&frame[..frame_len])
    }

    fn handle_tcp(&mut self, src_mac: [u8; 6], src_ip: [u8; 4], packet: &[u8], ihl: usize) {
        if packet.len() < ihl + 20 {
            return;
        }
        let tcp = &packet[ihl..];
        let src_port = u16::from_be_bytes([tcp[0], tcp[1]]);
        let dst_port = u16::from_be_bytes([tcp[2], tcp[3]]);
        let seq = u32::from_be_bytes([tcp[4], tcp[5], tcp[6], tcp[7]]);
        let ack = u32::from_be_bytes([tcp[8], tcp[9], tcp[10], tcp[11]]);
        let offset = ((tcp[12] >> 4) as usize) * 4;
        if offset < 20 || tcp.len() < offset {
            return;
        }
        let flags = tcp[13];
        let payload = &tcp[offset..];

        let mut client = TCP_CLIENT.lock();
        if !client.active {
            return;
        }
        if client.dst_ip != src_ip || client.src_port != dst_port || client.dst_port != src_port {
            return;
        }

        cache_arp(src_ip, src_mac);

        if !client.connected {
            if (flags & 0x12) == 0x12 && ack == client.seq.wrapping_add(1) {
                client.ack = seq.wrapping_add(1);
                client.seq = client.seq.wrapping_add(1);
                let _ = self.send_tcp_segment(
                    src_ip,
                    src_mac,
                    client.src_port,
                    client.dst_port,
                    client.seq,
                    client.ack,
                    0x10,
                    &[],
                );
                client.connected = true;
            }
            return;
        }

        let mut ack_advance = 0u32;

        if !payload.is_empty() {
            let copy_len = core::cmp::min(payload.len(), client.recv_buf.len().saturating_sub(client.recv_len));
            let start = client.recv_len;
            let end = start + copy_len;
            client.recv_buf[start..end].copy_from_slice(&payload[..copy_len]);
            client.recv_len += copy_len;
            ack_advance = copy_len as u32;
        }

        if (flags & 0x01) != 0 {
            ack_advance = ack_advance.wrapping_add(1);
        }

        if ack_advance > 0 {
            client.ack = seq.wrapping_add(ack_advance);
            let _ = self.send_tcp_segment(
                src_ip,
                src_mac,
                client.src_port,
                client.dst_port,
                client.seq,
                client.ack,
                0x10,
                &[],
            );
        }

        if (flags & 0x01) != 0 {
            client.connected = false;
            client.active = false;
        }
    }

    fn process_frame(&mut self, frame: &[u8]) {
        if frame.len() < 14 {
            return;
        }
        let src_mac = [frame[6], frame[7], frame[8], frame[9], frame[10], frame[11]];
        let eth_type = u16::from_be_bytes([frame[12], frame[13]]);

        match eth_type {
            ETH_TYPE_ARP => self.handle_arp(src_mac, &frame[14..]),
            ETH_TYPE_IPV4 => self.handle_ipv4(src_mac, &frame[14..]),
            _ => {}
        }
    }

    fn process_rx_queue(&mut self, max_frames: usize) {
        let mut processed = 0usize;
        while processed < max_frames {
            let packet = {
                let mut queue = RX_FRAME_QUEUE.lock();
                queue.pop_front()
            };

            match packet {
                Some(pkt) => {
                    self.process_frame(&pkt.data[..pkt.len]);
                    processed += 1;
                }
                None => break,
            }
        }
    }

    fn poll_rx(&mut self, max_packets: usize) {
        let mut processed = 0usize;
        while processed < max_packets {
            if io_read_u8(self.io_base, REG_CMD) & CMD_RX_EMPTY != 0 {
                break;
            }

            let base_ptr = self.rx_buffer_phys as *const u8;
            let status = unsafe {
                u16::from_le_bytes([
                    *base_ptr.add(self.rx_offset),
                    *base_ptr.add(self.rx_offset + 1),
                ])
            };
            let length = unsafe {
                u16::from_le_bytes([
                    *base_ptr.add(self.rx_offset + 2),
                    *base_ptr.add(self.rx_offset + 3),
                ])
            } as usize;

            if status == 0 || length < 4 || length > 2048 {
                RX_ERRORS.fetch_add(1, Ordering::Relaxed);

                let advance = if length >= 4 && length <= 2048 {
                    (length + 4 + 3) & !3
                } else {
                    4
                };

                self.rx_offset = (self.rx_offset + advance) % RX_BUFFER_SIZE;
                io_write_u16(self.io_base, REG_CAPR, (self.rx_offset as u16).wrapping_sub(16));
                processed += 1;
                continue;
            }

            let frame_len = length - 4;
            let mut frame = [0u8; 1600];
            if frame_len <= frame.len() {
                for i in 0..frame_len {
                    let idx = (self.rx_offset + 4 + i) % RX_BUFFER_SIZE;
                    frame[i] = unsafe { *base_ptr.add(idx) };
                }

                RX_PACKETS.fetch_add(1, Ordering::Relaxed);
                let mut queue = RX_FRAME_QUEUE.lock();
                if queue.len() >= 64 {
                    queue.pop_front();
                }
                queue.push_back(FramePacket {
                    len: frame_len,
                    data: frame,
                });
            }

            self.rx_offset = (self.rx_offset + length + 4 + 3) & !3;
            if self.rx_offset >= RX_BUFFER_SIZE {
                self.rx_offset -= RX_BUFFER_SIZE;
            }

            io_write_u16(self.io_base, REG_CAPR, (self.rx_offset as u16).wrapping_sub(16));

            let _ = io_read_u16(self.io_base, REG_CBR);
            processed += 1;
        }
    }

    fn handle_interrupt(&mut self) {
        let isr = io_read_u16(self.io_base, REG_ISR);
        if isr == 0 {
            return;
        }

        io_write_u16(self.io_base, REG_ISR, isr);

        if (isr & ISR_RX_ERR) != 0 {
            RX_ERRORS.fetch_add(1, Ordering::Relaxed);
        }
        if (isr & ISR_TX_ERR) != 0 {
            TX_ERRORS.fetch_add(1, Ordering::Relaxed);
        }
    }
}

pub fn init() {
    match Rtl8139::init() {
        Ok(driver) => {
            crate::kprintln!(
                "[NET] RTL8139 up: io={:#x} irq={} mac={:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                driver.io_base,
                driver.irq,
                driver.mac[0],
                driver.mac[1],
                driver.mac[2],
                driver.mac[3],
                driver.mac[4],
                driver.mac[5]
            );
            crate::kprintln!(
                "[NET] IPv4={}.{}.{}.{} gw={}.{}.{}.{}",
                config_ip()[0],
                config_ip()[1],
                config_ip()[2],
                config_ip()[3],
                config_gateway()[0],
                config_gateway()[1],
                config_gateway()[2],
                config_gateway()[3]
            );
            *RTL8139.lock() = Some(driver);
        }
        Err(err) => {
            crate::kprintln!("[NET] No RTL8139 network device: {}", err);
        }
    }
}

pub fn is_available() -> bool {
    RTL8139.lock().is_some()
}

pub fn handle_interrupt() {
    if let Some(mut guard) = RTL8139.try_lock() {
        if let Some(ref mut nic) = *guard {
            nic.handle_interrupt();
        }
    }
}

pub fn poll() {
    if let Some(ref mut nic) = *RTL8139.lock() {
        nic.poll_rx(16);
        nic.process_rx_queue(16);
    }
}

pub fn mac() -> Option<[u8; 6]> {
    RTL8139.lock().as_ref().map(|nic| nic.mac)
}

pub fn ip() -> [u8; 4] {
    config_ip()
}

pub fn netmask() -> [u8; 4] {
    NET_CONFIG.lock().netmask
}

pub fn gateway() -> [u8; 4] {
    config_gateway()
}

pub fn dns_server() -> [u8; 4] {
    config_dns()
}

pub fn set_ip(new_ip: [u8; 4]) {
    NET_CONFIG.lock().ip = new_ip;
}

pub fn set_netmask(new_netmask: [u8; 4]) {
    NET_CONFIG.lock().netmask = new_netmask;
}

pub fn set_gateway(new_gateway: [u8; 4]) {
    NET_CONFIG.lock().gateway = new_gateway;
}

pub fn set_dns(new_dns: [u8; 4]) {
    NET_CONFIG.lock().dns = new_dns;
}

pub fn request_arp(ip: [u8; 4]) -> Result<(), &'static str> {
    if let Some(ref mut nic) = *RTL8139.lock() {
        nic.send_arp_request(ip);
        Ok(())
    } else {
        Err("network unavailable")
    }
}

pub fn ping(ip: [u8; 4]) -> Result<(), &'static str> {
    if let Some(ref mut nic) = *RTL8139.lock() {
        let next_hop = route_next_hop(ip);
        let mac = resolve_arp(next_hop, 250)?;
        let seq = PING_SEQ.fetch_add(1, Ordering::Relaxed);
        nic.send_icmp_echo(ip, mac, 0xC077, seq);
        Ok(())
    } else {
        Err("network unavailable")
    }
}

pub fn tcp_connect(ip: [u8; 4], port: u16) -> Result<(), &'static str> {
    let next_hop = route_next_hop(ip);
    let dst_mac = resolve_arp(next_hop, 300)?;
    let mut nic_guard = RTL8139.lock();
    let nic = nic_guard.as_mut().ok_or("network unavailable")?;

    let src_port = TCP_SRC_PORT_SEQ.fetch_add(1, Ordering::Relaxed);
    let seq = TCP_SEQ_GEN.fetch_add(0x101, Ordering::Relaxed);

    {
        let mut client = TCP_CLIENT.lock();
        client.active = true;
        client.connected = false;
        client.dst_ip = ip;
        client.dst_port = port;
        client.src_port = src_port;
        client.seq = seq;
        client.ack = 0;
        client.recv_len = 0;
    }

    nic.send_tcp_segment(ip, dst_mac, src_port, port, seq, 0, 0x02, &[])?;
    Ok(())
}

pub fn tcp_is_connected() -> bool {
    TCP_CLIENT.lock().connected
}

pub fn tcp_send(data: &[u8]) -> Result<(), &'static str> {
    let (ip, port, src_port, seq, ack) = {
        let client = TCP_CLIENT.lock();
        if !client.active || !client.connected {
            return Err("tcp not connected");
        }
        (client.dst_ip, client.dst_port, client.src_port, client.seq, client.ack)
    };

    let next_hop = route_next_hop(ip);
    let dst_mac = resolve_arp(next_hop, 300)?;
    let mut nic_guard = RTL8139.lock();
    let nic = nic_guard.as_mut().ok_or("network unavailable")?;

    nic.send_tcp_segment(ip, dst_mac, src_port, port, seq, ack, 0x18, data)?;

    let mut client = TCP_CLIENT.lock();
    client.seq = client.seq.wrapping_add(data.len() as u32);
    Ok(())
}

pub fn tcp_read() -> Option<([u8; 1024], usize)> {
    let mut client = TCP_CLIENT.lock();
    if client.recv_len == 0 {
        return None;
    }
    let mut out = [0u8; 1024];
    let recv_len = client.recv_len;
    let len = core::cmp::min(recv_len, out.len());
    out[..len].copy_from_slice(&client.recv_buf[..len]);
    if len < recv_len {
        client.recv_buf.copy_within(len..recv_len, 0);
    }
    client.recv_len = recv_len - len;
    Some((out, len))
}

pub fn tcp_read_into(buf: &mut [u8]) -> usize {
    if buf.is_empty() {
        return 0;
    }

    let mut client = TCP_CLIENT.lock();
    if client.recv_len == 0 {
        return 0;
    }

    let recv_len = client.recv_len;
    let len = core::cmp::min(recv_len, buf.len());
    buf[..len].copy_from_slice(&client.recv_buf[..len]);
    if len < recv_len {
        client.recv_buf.copy_within(len..recv_len, 0);
    }
    client.recv_len = recv_len - len;
    len
}

pub fn tcp_close() -> Result<(), &'static str> {
    let (ip, port, src_port, seq, ack) = {
        let client = TCP_CLIENT.lock();
        if !client.active {
            return Ok(());
        }
        (client.dst_ip, client.dst_port, client.src_port, client.seq, client.ack)
    };

    let next_hop = route_next_hop(ip);
    let dst_mac = resolve_arp(next_hop, 300)?;
    let mut nic_guard = RTL8139.lock();
    let nic = nic_guard.as_mut().ok_or("network unavailable")?;
    nic.send_tcp_segment(ip, dst_mac, src_port, port, seq, ack, 0x11, &[])?;

    TCP_CLIENT.lock().reset();
    Ok(())
}

pub fn udp_send(dst_ip: [u8; 4], src_port: u16, dst_port: u16, payload: &[u8]) -> Result<(), &'static str> {
    let next_hop = route_next_hop(dst_ip);
    let dst_mac = resolve_arp(next_hop, 300)?;

    let mut nic_guard = RTL8139.lock();
    let nic = nic_guard.as_mut().ok_or("network unavailable")?;
    nic.send_udp_packet(dst_ip, dst_mac, src_port, dst_port, payload)
}

pub fn udp_recv() -> Option<([u8; 4], u16, u16, [u8; 1024], usize)> {
    let mut queue = UDP_RX_QUEUE.lock();
    queue.pop_front().map(|pkt| (pkt.src_ip, pkt.src_port, pkt.dst_port, pkt.payload, pkt.len))
}

fn parse_dhcp_options(options: &[u8]) -> ([u8; 4], [u8; 4], [u8; 4], [u8; 4], u8) {
    let mut subnet = [0u8; 4];
    let mut router = [0u8; 4];
    let mut dns = [0u8; 4];
    let mut server = [0u8; 4];
    let mut msg_type = 0u8;

    let mut idx = 0usize;
    while idx < options.len() {
        let tag = options[idx];
        idx += 1;

        if tag == 255 {
            break;
        }
        if tag == 0 {
            continue;
        }
        if idx >= options.len() {
            break;
        }

        let len = options[idx] as usize;
        idx += 1;
        if idx + len > options.len() {
            break;
        }

        let data = &options[idx..(idx + len)];
        match tag {
            1 if len >= 4 => subnet.copy_from_slice(&data[0..4]),
            3 if len >= 4 => router.copy_from_slice(&data[0..4]),
            6 if len >= 4 => dns.copy_from_slice(&data[0..4]),
            53 if len >= 1 => msg_type = data[0],
            54 if len >= 4 => server.copy_from_slice(&data[0..4]),
            _ => {}
        }

        idx += len;
    }

    (subnet, router, dns, server, msg_type)
}

pub fn dhcp_configure() -> Result<(), &'static str> {
    let mut discover = [0u8; 300];
    let xid = DHCP_XID_GEN.fetch_add(1, Ordering::Relaxed);

    discover[0] = 1;
    discover[1] = 1;
    discover[2] = 6;
    discover[3] = 0;
    discover[4..8].copy_from_slice(&xid.to_be_bytes());
    discover[10..12].copy_from_slice(&0x8000u16.to_be_bytes());
    if let Some(hw) = mac() {
        discover[28..34].copy_from_slice(&hw);
    }
    discover[236..240].copy_from_slice(&[99, 130, 83, 99]);
    discover[240..243].copy_from_slice(&[53, 1, 1]);
    discover[243..249].copy_from_slice(&[55, 4, 1, 3, 6, 15]);
    discover[249] = 255;

    {
        let mut nic_guard = RTL8139.lock();
        let nic = nic_guard.as_mut().ok_or("network unavailable")?;
        nic.send_udp_broadcast_packet(68, 67, &discover[..250])?;
    }

    let mut offered_ip = [0u8; 4];
    let mut server_ip = [0u8; 4];
    let mut offer_mask = [0u8; 4];
    let mut offer_gw = [0u8; 4];
    let mut offer_dns = [0u8; 4];

    let offer_deadline = crate::proc::scheduler::ticks() + 3000;
    while crate::proc::scheduler::ticks() < offer_deadline {
        poll();
        if let Some((_src_ip, src_port, dst_port, payload, len)) = udp_recv() {
            if src_port != 67 || dst_port != 68 || len < 244 {
                continue;
            }
            if payload[236..240] != [99, 130, 83, 99] {
                continue;
            }
            let rx_xid = u32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]]);
            if rx_xid != xid {
                continue;
            }

            let yiaddr = [payload[16], payload[17], payload[18], payload[19]];
            let (mask, gw, dns, server, msg) = parse_dhcp_options(&payload[240..len]);
            if msg == 2 {
                offered_ip = yiaddr;
                offer_mask = mask;
                offer_gw = gw;
                offer_dns = dns;
                server_ip = server;
                break;
            }
        }
        crate::arch::halt();
    }

    if offered_ip == [0; 4] {
        return Err("dhcp offer timeout");
    }

    let mut request = [0u8; 320];
    request[0] = 1;
    request[1] = 1;
    request[2] = 6;
    request[3] = 0;
    request[4..8].copy_from_slice(&xid.to_be_bytes());
    request[10..12].copy_from_slice(&0x8000u16.to_be_bytes());
    if let Some(hw) = mac() {
        request[28..34].copy_from_slice(&hw);
    }
    request[236..240].copy_from_slice(&[99, 130, 83, 99]);

    let mut idx = 240usize;
    request[idx..(idx + 3)].copy_from_slice(&[53, 1, 3]);
    idx += 3;
    request[idx..(idx + 6)].copy_from_slice(&[50, 4, offered_ip[0], offered_ip[1], offered_ip[2], offered_ip[3]]);
    idx += 6;
    if server_ip != [0; 4] {
        request[idx..(idx + 6)].copy_from_slice(&[54, 4, server_ip[0], server_ip[1], server_ip[2], server_ip[3]]);
        idx += 6;
    }
    request[idx..(idx + 6)].copy_from_slice(&[55, 4, 1, 3, 6, 15]);
    idx += 6;
    request[idx] = 255;
    idx += 1;

    {
        let mut nic_guard = RTL8139.lock();
        let nic = nic_guard.as_mut().ok_or("network unavailable")?;
        nic.send_udp_broadcast_packet(68, 67, &request[..idx])?;
    }

    let ack_deadline = crate::proc::scheduler::ticks() + 3000;
    while crate::proc::scheduler::ticks() < ack_deadline {
        poll();
        if let Some((_src_ip, src_port, dst_port, payload, len)) = udp_recv() {
            if src_port != 67 || dst_port != 68 || len < 244 {
                continue;
            }
            if payload[236..240] != [99, 130, 83, 99] {
                continue;
            }
            let rx_xid = u32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]]);
            if rx_xid != xid {
                continue;
            }

            let yiaddr = [payload[16], payload[17], payload[18], payload[19]];
            let (mask, gw, dns, _server, msg) = parse_dhcp_options(&payload[240..len]);
            if msg == 5 {
                set_ip(yiaddr);
                if mask != [0; 4] {
                    set_netmask(mask);
                } else if offer_mask != [0; 4] {
                    set_netmask(offer_mask);
                }
                if gw != [0; 4] {
                    set_gateway(gw);
                } else if offer_gw != [0; 4] {
                    set_gateway(offer_gw);
                }
                if dns != [0; 4] {
                    set_dns(dns);
                } else if offer_dns != [0; 4] {
                    set_dns(offer_dns);
                }
                return Ok(());
            }
        }
        crate::arch::halt();
    }

    Err("dhcp ack timeout")
}

fn skip_dns_name(packet: &[u8], mut idx: usize) -> Option<usize> {
    if idx >= packet.len() {
        return None;
    }

    loop {
        if idx >= packet.len() {
            return None;
        }
        let len = packet[idx];
        if len == 0 {
            return Some(idx + 1);
        }
        if (len & 0xC0) == 0xC0 {
            if idx + 1 >= packet.len() {
                return None;
            }
            return Some(idx + 2);
        }
        idx += 1 + len as usize;
    }
}

fn scan_dns_rrs_for_a(packet: &[u8], mut cur: usize, count: usize) -> Result<(usize, Option<[u8; 4]>), &'static str> {
    for _ in 0..count {
        cur = skip_dns_name(packet, cur).ok_or("dns malformed answer name")?;
        if cur + 10 > packet.len() {
            return Err("dns malformed answer header");
        }

        let rr_type = u16::from_be_bytes([packet[cur], packet[cur + 1]]);
        let rr_class = u16::from_be_bytes([packet[cur + 2], packet[cur + 3]]);
        let rdlen = u16::from_be_bytes([packet[cur + 8], packet[cur + 9]]) as usize;
        cur += 10;

        if cur + rdlen > packet.len() {
            return Err("dns malformed rdata");
        }

        if rr_type == 1 && rr_class == 1 && rdlen == 4 {
            return Ok((cur + rdlen, Some([packet[cur], packet[cur + 1], packet[cur + 2], packet[cur + 3]])));
        }

        cur += rdlen;
    }

    Ok((cur, None))
}

pub fn dns_resolve_a(host: &str) -> Result<[u8; 4], &'static str> {
    if host.is_empty() || host.len() > 240 {
        return Err("invalid host name");
    }

    let primary_dns = dns_server();
    let gateway_dns = gateway();
    let mut dns_targets = [[0u8; 4]; 2];
    let mut dns_target_count = 0usize;

    if primary_dns != [0, 0, 0, 0] {
        dns_targets[dns_target_count] = primary_dns;
        dns_target_count += 1;
    }
    if gateway_dns != [0, 0, 0, 0] && gateway_dns != primary_dns {
        dns_targets[dns_target_count] = gateway_dns;
        dns_target_count += 1;
    }
    if dns_target_count == 0 {
        dns_targets[0] = DEFAULT_DNS;
        dns_target_count = 1;
    }

    let mut query = [0u8; 512];
    query[2..4].copy_from_slice(&0x0100u16.to_be_bytes());
    query[4..6].copy_from_slice(&1u16.to_be_bytes());

    let mut idx = 12usize;
    for label in host.split('.') {
        if label.is_empty() || label.len() > 63 {
            return Err("invalid host label");
        }
        query[idx] = label.len() as u8;
        idx += 1;
        let label_bytes = label.as_bytes();
        query[idx..(idx + label_bytes.len())].copy_from_slice(label_bytes);
        idx += label_bytes.len();
    }
    query[idx] = 0;
    idx += 1;
    query[idx..(idx + 2)].copy_from_slice(&1u16.to_be_bytes());
    idx += 2;
    query[idx..(idx + 2)].copy_from_slice(&1u16.to_be_bytes());
    idx += 2;

    for dns in dns_targets.iter().copied().take(dns_target_count) {
        for _attempt in 0..3 {
            let query_id = DNS_ID_GEN.fetch_add(1, Ordering::Relaxed);
            let src_port = 53000u16.wrapping_add(query_id % 1000);
            query[0..2].copy_from_slice(&query_id.to_be_bytes());

            let _ = request_arp(dns);
            let _ = request_arp(gateway());
            udp_send(dns, src_port, 53, &query[..idx])?;

            let deadline = crate::proc::scheduler::ticks() + 1200;
            while crate::proc::scheduler::ticks() < deadline {
                poll();
                if let Some((_src_ip, src_port_rx, dst_port, payload, len)) = udp_recv() {
                    if src_port_rx != 53 || dst_port != src_port || len < 12 {
                        continue;
                    }
                    let resp_id = u16::from_be_bytes([payload[0], payload[1]]);
                    if resp_id != query_id {
                        continue;
                    }

                    let qdcount = u16::from_be_bytes([payload[4], payload[5]]) as usize;
                    let ancount = u16::from_be_bytes([payload[6], payload[7]]) as usize;
                    let nscount = u16::from_be_bytes([payload[8], payload[9]]) as usize;
                    let arcount = u16::from_be_bytes([payload[10], payload[11]]) as usize;

                    let packet = &payload[..len];
                    let mut cur = 12usize;
                    for _ in 0..qdcount {
                        cur = skip_dns_name(packet, cur).ok_or("dns malformed question")?;
                        if cur + 4 > len {
                            return Err("dns malformed question tail");
                        }
                        cur += 4;
                    }

                    let (next_cur, answer_a) = scan_dns_rrs_for_a(packet, cur, ancount)?;
                    if let Some(ip) = answer_a {
                        return Ok(ip);
                    }

                    let (next_cur, _ns_a) = scan_dns_rrs_for_a(packet, next_cur, nscount)?;
                    let (_next_cur, additional_a) = scan_dns_rrs_for_a(packet, next_cur, arcount)?;
                    if let Some(ip) = additional_a {
                        return Ok(ip);
                    }
                }
                crate::arch::halt();
            }
        }
    }

    Err("dns timeout/no A record")
}

pub fn stats() -> (u64, u64, u64, u64, u64, u64) {
    (
        RX_PACKETS.load(Ordering::Relaxed),
        TX_PACKETS.load(Ordering::Relaxed),
        RX_ERRORS.load(Ordering::Relaxed),
        TX_ERRORS.load(Ordering::Relaxed),
        ICMP_ECHO_RX.load(Ordering::Relaxed),
        ICMP_ECHO_TX.load(Ordering::Relaxed),
    )
}

pub fn arp_entries() -> [([u8; 4], [u8; 6], bool); 8] {
    let cache = ARP_CACHE.lock();
    let mut entries = [([0u8; 4], [0u8; 6], false); 8];
    for (idx, entry) in cache.iter().enumerate() {
        entries[idx] = (entry.ip, entry.mac, entry.valid);
    }
    entries
}
