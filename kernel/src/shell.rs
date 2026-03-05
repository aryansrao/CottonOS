//! CottonOS Kernel Shell
//!
//! Simple interactive shell for testing and debugging

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use crate::kprint;
use crate::kprintln;

/// Current working directory
static mut CWD: Option<String> = None;

/// Whether disk is available
static mut HAS_DISK: bool = false;

/// Get current working directory
pub fn get_cwd() -> String {
    unsafe {
        CWD.clone().unwrap_or_else(|| String::from("/"))
    }
}

fn set_cwd(path: String) {
    unsafe {
        CWD = Some(path);
    }
}

/// Check if disk is available
fn has_disk() -> bool {
    unsafe { HAS_DISK }
}

/// Set disk availability
fn set_has_disk(val: bool) {
    unsafe { HAS_DISK = val; }
}

/// Resolve a path (handle relative paths)
pub fn resolve_path(path: &str) -> String {
    if path.starts_with('/') {
        String::from(path)
    } else {
        let cwd = get_cwd();
        if cwd == "/" {
            format!("/{}", path)
        } else {
            format!("{}/{}", cwd, path)
        }
    }
}

/// Execute a shell command and return output as String (for GUI terminal)
pub fn execute_command(line: &str) -> String {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.is_empty() {
        return String::new();
    }
    
    let cmd = parts[0];
    let args = &parts[1..];
    
    match cmd {
        "help" => {
            if args.is_empty() {
                String::from("Commands: help, clear, info, mem, df, ps, uptime, echo, sync, reboot, halt\nNetwork:  net, netstats, arptable, arp, ping, dhcp, dns, setip, setmask, setgw, setdns\nTCP:      tcpconnect, tcpsend, tcprecv, tcpclose, httpget, httpsget\nUDP:      udpsend, udprecv\nFiles:    ls, cd, pwd, cat, touch, mkdir, rm, write\n\nFiles are stored persistently on disk (CottonFS).")
            } else {
                exec_help_detail(args[0])
            }
        }
        "clear" => String::from("\x1b[CLEAR]"),
        "info" => exec_info(),
        "mem" => exec_mem(),
        "df" => exec_df(),
        "sync" => exec_sync(),
        "ps" => exec_ps(),
        "uptime" => exec_uptime(),
        "echo" => args.join(" "),
        "net" => exec_net(),
        "netstats" => exec_netstats(),
        "arptable" => exec_arptable(),
        "arp" => exec_arp(args),
        "ping" => exec_ping(args),
        "dhcp" => exec_dhcp(),
        "dns" => exec_dns(args),
        "setip" => exec_setip(args),
        "setmask" => exec_setmask(args),
        "setgw" => exec_setgw(args),
        "setdns" => exec_setdns(args),
        "tcpconnect" => exec_tcpconnect(args),
        "tcpsend" => exec_tcpsend(args),
        "tcprecv" => exec_tcprecv(),
        "tcpclose" => exec_tcpclose(),
        "httpget" => exec_httpget(args),
        "httpsget" => exec_httpsget(args),
        "udpsend" => exec_udpsend(args),
        "udprecv" => exec_udprecv(),
        "panic" => { panic!("User-triggered panic"); }
        "reboot" => { cmd_reboot(); String::from("Rebooting...") }
        "halt" => { cmd_halt(); String::from("System halted.") }
        "ls" => exec_ls(args),
        "cd" => exec_cd(args),
        "pwd" => get_cwd(),
        "cat" => exec_cat(args),
        "touch" => exec_touch(args),
        "mkdir" => exec_mkdir(args),
        "rm" => exec_rm(args),
        "write" => exec_write(args),
        _ => format!("Unknown command: '{}'. Type 'help'.", cmd),
    }
}

fn exec_help_detail(cmd: &str) -> String {
    match cmd {
        "ls" => String::from("ls [path] - List directory contents"),
        "cd" => String::from("cd <path> - Change directory"),
        "pwd" => String::from("pwd - Print working directory"),
        "cat" => String::from("cat <file> - Display file contents"),
        "touch" => String::from("touch <file> - Create empty file"),
        "mkdir" => String::from("mkdir <dir> - Create directory"),
        "rm" => String::from("rm <file> - Remove file or empty directory"),
        "write" => String::from("write <file> <text> - Write text to file"),
        "df" => String::from("df - Show disk space usage (CottonFS)"),
        "sync" => String::from("sync - Force sync all data to disk"),
        "info" => String::from("info - Show system information"),
        "mem" => String::from("mem - Show memory statistics"),
        "ps" => String::from("ps - List running processes"),
        "uptime" => String::from("uptime - Show system uptime"),
        "echo" => String::from("echo <text> - Print text"),
        "net" => String::from("net - Show network interface information"),
        "netstats" => String::from("netstats - Show network packet counters"),
        "arptable" => String::from("arptable - Show ARP cache"),
        "arp" => String::from("arp <ip> - Send ARP request to host"),
        "ping" => String::from("ping <ip> - Send ICMP echo request"),
        "dhcp" => String::from("dhcp - Request IPv4 config via DHCP"),
        "dns" => String::from("dns <host> - Resolve hostname to IPv4"),
        "setip" => String::from("setip <ip> - Set interface IPv4 address"),
        "setmask" => String::from("setmask <mask> - Set netmask"),
        "setgw" => String::from("setgw <ip> - Set default gateway"),
        "setdns" => String::from("setdns <ip> - Set DNS server"),
        "tcpconnect" => String::from("tcpconnect <ip> <port> - Open TCP connection"),
        "tcpsend" => String::from("tcpsend <text> - Send TCP payload on active connection"),
        "tcprecv" => String::from("tcprecv - Read buffered TCP payload"),
        "tcpclose" => String::from("tcpclose - Close active TCP connection"),
        "httpget" => String::from("httpget <host-or-ip> [path] - Basic HTTP GET over TCP (no HTTPS)"),
        "httpsget" => String::from("httpsget <host-or-ip> [path] - HTTPS GET over in-kernel TLS"),
        "udpsend" => String::from("udpsend <ip> <src_port> <dst_port> <text> - Send UDP datagram"),
        "udprecv" => String::from("udprecv - Receive one UDP datagram"),
        "clear" => String::from("clear - Clear the screen"),
        "reboot" => String::from("reboot - Restart the system"),
        "halt" => String::from("halt - Stop the CPU"),
        _ => format!("Unknown command: {}", cmd),
    }
}

fn parse_ipv4(s: &str) -> Option<[u8; 4]> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let mut out = [0u8; 4];
    for (idx, part) in parts.iter().enumerate() {
        out[idx] = part.parse::<u8>().ok()?;
    }
    Some(out)
}

fn fmt_ipv4(ip: [u8; 4]) -> String {
    format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3])
}

fn fmt_mac(mac: [u8; 6]) -> String {
    format!(
        "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    )
}

fn exec_net() -> String {
    if !crate::drivers::network::is_available() {
        return String::from("Network: unavailable (RTL8139 not detected)");
    }

    let mac = crate::drivers::network::mac().unwrap_or([0; 6]);
    let ip = crate::drivers::network::ip();
    let mask = crate::drivers::network::netmask();
    let gw = crate::drivers::network::gateway();
    let dns = crate::drivers::network::dns_server();

    format!(
        "Network interface: rtl8139\n  MAC: {}\n  IPv4: {}\n  Netmask: {}\n  Gateway: {}\n  DNS: {}",
        fmt_mac(mac),
        fmt_ipv4(ip),
        fmt_ipv4(mask),
        fmt_ipv4(gw),
        fmt_ipv4(dns)
    )
}

fn exec_netstats() -> String {
    let (rx, tx, rx_err, tx_err, icmp_rx, icmp_tx) = crate::drivers::network::stats();
    format!(
        "Network counters:\n  RX packets: {}\n  TX packets: {}\n  RX errors:  {}\n  TX errors:  {}\n  ICMP echo rx: {}\n  ICMP echo tx: {}",
        rx, tx, rx_err, tx_err, icmp_rx, icmp_tx
    )
}

fn exec_arptable() -> String {
    let entries = crate::drivers::network::arp_entries();
    let mut out = String::from("ARP table:");
    let mut any = false;
    for (ip, mac, valid) in entries {
        if valid {
            any = true;
            out.push_str(&format!("\n  {:15} -> {}", fmt_ipv4(ip), fmt_mac(mac)));
        }
    }
    if !any {
        out.push_str("\n  (empty)");
    }
    out
}

fn exec_arp(args: &[&str]) -> String {
    if args.is_empty() {
        return String::from("arp: usage: arp <ip>");
    }
    let ip = match parse_ipv4(args[0]) {
        Some(ip) => ip,
        None => return String::from("arp: invalid IPv4 address"),
    };

    match crate::drivers::network::request_arp(ip) {
        Ok(()) => format!("ARP request sent for {}", fmt_ipv4(ip)),
        Err(e) => format!("arp: {}", e),
    }
}

fn exec_ping(args: &[&str]) -> String {
    if args.is_empty() {
        return String::from("ping: usage: ping <ip>");
    }
    let ip = match parse_ipv4(args[0]) {
        Some(ip) => ip,
        None => return String::from("ping: invalid IPv4 address"),
    };

    match crate::drivers::network::ping(ip) {
        Ok(()) => format!("ICMP echo request sent to {}", fmt_ipv4(ip)),
        Err(e) => format!("ping: {}", e),
    }
}

fn exec_setip(args: &[&str]) -> String {
    if args.is_empty() {
        return String::from("setip: usage: setip <ipv4>");
    }
    let ip = match parse_ipv4(args[0]) {
        Some(ip) => ip,
        None => return String::from("setip: invalid IPv4 address"),
    };
    crate::drivers::network::set_ip(ip);
    format!("IP set to {}", fmt_ipv4(ip))
}

fn exec_setmask(args: &[&str]) -> String {
    if args.is_empty() {
        return String::from("setmask: usage: setmask <mask>");
    }
    let mask = match parse_ipv4(args[0]) {
        Some(mask) => mask,
        None => return String::from("setmask: invalid netmask"),
    };
    crate::drivers::network::set_netmask(mask);
    format!("Netmask set to {}", fmt_ipv4(mask))
}

fn exec_setgw(args: &[&str]) -> String {
    if args.is_empty() {
        return String::from("setgw: usage: setgw <gateway>");
    }
    let gw = match parse_ipv4(args[0]) {
        Some(gw) => gw,
        None => return String::from("setgw: invalid gateway"),
    };
    crate::drivers::network::set_gateway(gw);
    format!("Gateway set to {}", fmt_ipv4(gw))
}

fn exec_setdns(args: &[&str]) -> String {
    if args.is_empty() {
        return String::from("setdns: usage: setdns <dns>");
    }
    let dns = match parse_ipv4(args[0]) {
        Some(dns) => dns,
        None => return String::from("setdns: invalid dns"),
    };
    crate::drivers::network::set_dns(dns);
    format!("DNS set to {}", fmt_ipv4(dns))
}

fn exec_dhcp() -> String {
    match crate::drivers::network::dhcp_configure() {
        Ok(()) => {
            let ip = crate::drivers::network::ip();
            let mask = crate::drivers::network::netmask();
            let gw = crate::drivers::network::gateway();
            let dns = crate::drivers::network::dns_server();
            format!(
                "DHCP configured:\n  IPv4: {}\n  Netmask: {}\n  Gateway: {}\n  DNS: {}",
                fmt_ipv4(ip),
                fmt_ipv4(mask),
                fmt_ipv4(gw),
                fmt_ipv4(dns)
            )
        }
        Err(e) => format!("dhcp: {}", e),
    }
}

fn exec_dns(args: &[&str]) -> String {
    if args.is_empty() {
        return String::from("dns: usage: dns <hostname>");
    }
    match crate::drivers::network::dns_resolve_a(args[0]) {
        Ok(ip) => format!("{} -> {}", args[0], fmt_ipv4(ip)),
        Err(e) => format!("dns: {}", e),
    }
}

fn exec_tcpconnect(args: &[&str]) -> String {
    if args.len() < 2 {
        return String::from("tcpconnect: usage: tcpconnect <ip> <port>");
    }
    let ip = match parse_ipv4(args[0]) {
        Some(ip) => ip,
        None => return String::from("tcpconnect: invalid IPv4 address"),
    };
    let port = match args[1].parse::<u16>() {
        Ok(port) if port > 0 => port,
        _ => return String::from("tcpconnect: invalid port"),
    };

    if crate::drivers::network::request_arp(ip).is_err() {
        let _ = crate::drivers::network::request_arp(crate::drivers::network::gateway());
    }

    match crate::drivers::network::tcp_connect(ip, port) {
        Ok(()) => String::from("TCP SYN sent; wait and run tcprecv/netstats"),
        Err(e) => format!("tcpconnect: {}", e),
    }
}

fn exec_tcpsend(args: &[&str]) -> String {
    if args.is_empty() {
        return String::from("tcpsend: usage: tcpsend <text>");
    }
    let text = args.join(" ");
    match crate::drivers::network::tcp_send(text.as_bytes()) {
        Ok(()) => format!("Sent {} bytes", text.len()),
        Err(e) => format!("tcpsend: {}", e),
    }
}

fn exec_tcprecv() -> String {
    crate::drivers::network::poll();
    match crate::drivers::network::tcp_read() {
        Some((buf, len)) => String::from_utf8_lossy(&buf[..len]).into_owned(),
        None => String::from("(no tcp data)"),
    }
}

fn exec_tcpclose() -> String {
    match crate::drivers::network::tcp_close() {
        Ok(()) => String::from("TCP connection closed"),
        Err(e) => format!("tcpclose: {}", e),
    }
}

fn exec_httpget(args: &[&str]) -> String {
    if args.is_empty() {
        return String::from("httpget: usage: httpget <host-or-ip> [path]");
    }
    let host = args[0];
    let ip = match parse_ipv4(host) {
        Some(ip) => ip,
        None => match crate::drivers::network::dns_resolve_a(host) {
            Ok(ip) => ip,
            Err(e) => return format!("httpget: dns resolve failed: {}", e),
        },
    };
    let path = if args.len() > 1 { args[1] } else { "/" };

    let _ = crate::drivers::network::request_arp(ip);
    let _ = crate::drivers::network::request_arp(crate::drivers::network::gateway());

    if let Err(e) = crate::drivers::network::tcp_connect(ip, 80) {
        return format!("httpget: {}", e);
    }

    let start = crate::proc::scheduler::ticks();
    while !crate::drivers::network::tcp_is_connected() && (crate::proc::scheduler::ticks() - start) < 1500 {
        crate::drivers::network::poll();
        crate::arch::halt();
    }
    if !crate::drivers::network::tcp_is_connected() {
        let _ = crate::drivers::network::tcp_close();
        return String::from("httpget: tcp connect timeout");
    }

    let req = format!("GET {} HTTP/1.0\r\nHost: {}\r\nUser-Agent: CottonOS\r\n\r\n", path, host);
    if let Err(e) = crate::drivers::network::tcp_send(req.as_bytes()) {
        let _ = crate::drivers::network::tcp_close();
        return format!("httpget send failed: {}", e);
    }

    let mut out = String::new();
    let read_start = crate::proc::scheduler::ticks();
    let mut last_data_tick = read_start;
    let mut saw_data = false;

    while (crate::proc::scheduler::ticks() - read_start) < 4000 {
        crate::drivers::network::poll();
        if let Some((buf, len)) = crate::drivers::network::tcp_read() {
            out.push_str(&String::from_utf8_lossy(&buf[..len]));
            saw_data = true;
            last_data_tick = crate::proc::scheduler::ticks();
        }

        if !crate::drivers::network::tcp_is_connected() {
            break;
        }

        if saw_data && (crate::proc::scheduler::ticks() - last_data_tick) > 250 {
            break;
        }

        crate::arch::halt();
    }

    let _ = crate::drivers::network::tcp_close();
    if out.is_empty() {
        String::from("httpget: no response")
    } else {
        out
    }
}

fn exec_httpsget(args: &[&str]) -> String {
    if args.is_empty() {
        return String::from("httpsget: usage: httpsget <host-or-ip> [path]");
    }

    let host = args[0];
    let ip = match parse_ipv4(host) {
        Some(ip) => ip,
        None => match crate::drivers::network::dns_resolve_a(host) {
            Ok(ip) => ip,
            Err(e) => return format!("httpsget: dns resolve failed: {}", e),
        },
    };

    let path = if args.len() > 1 { args[1] } else { "/" };
    let _ = crate::drivers::network::request_arp(ip);
    let _ = crate::drivers::network::request_arp(crate::drivers::network::gateway());

    match crate::crypto::tls::https_get(host, ip, path) {
        Ok(resp) => resp,
        Err(e) => e,
    }
}

fn exec_udpsend(args: &[&str]) -> String {
    if args.len() < 4 {
        return String::from("udpsend: usage: udpsend <ip> <src_port> <dst_port> <text>");
    }

    let ip = match parse_ipv4(args[0]) {
        Some(ip) => ip,
        None => return String::from("udpsend: invalid IPv4 address"),
    };
    let src_port = match args[1].parse::<u16>() {
        Ok(port) if port > 0 => port,
        _ => return String::from("udpsend: invalid src_port"),
    };
    let dst_port = match args[2].parse::<u16>() {
        Ok(port) if port > 0 => port,
        _ => return String::from("udpsend: invalid dst_port"),
    };

    let payload = args[3..].join(" ");
    let _ = crate::drivers::network::request_arp(ip);
    let _ = crate::drivers::network::request_arp(crate::drivers::network::gateway());

    match crate::drivers::network::udp_send(ip, src_port, dst_port, payload.as_bytes()) {
        Ok(()) => format!("UDP sent ({} bytes) to {}:{}", payload.len(), fmt_ipv4(ip), dst_port),
        Err(e) => format!("udpsend: {}", e),
    }
}

fn exec_udprecv() -> String {
    crate::drivers::network::poll();
    match crate::drivers::network::udp_recv() {
        Some((src_ip, src_port, dst_port, payload, len)) => {
            let text = String::from_utf8_lossy(&payload[..len]).into_owned();
            format!(
                "UDP {}:{} -> local:{} ({} bytes): {}",
                fmt_ipv4(src_ip),
                src_port,
                dst_port,
                len,
                text
            )
        }
        None => String::from("(no udp datagrams)"),
    }
}

fn exec_info() -> String {
    format!("+--------------------------------------------+\n|           CottonOS System Info             |\n+--------------------------------------------+\n|  Kernel Version: {}                     |\n|  Architecture:   {:?}                  |\n|  Filesystem:     CottonFS (persistent)    |\n+--------------------------------------------+",
        crate::KERNEL_VERSION, crate::Architecture::current())
}

fn exec_mem() -> String {
    let (total, used, free) = crate::mm::physical::stats();
    format!("Memory Statistics:\n  Total:     {} KB ({} MB)\n  Used:      {} KB ({} MB)\n  Free:      {} KB ({} MB)\n  Usage:     {}%",
        total / 1024, total / (1024 * 1024),
        used / 1024, used / (1024 * 1024),
        free / 1024, free / (1024 * 1024),
        if total > 0 { (used * 100) / total } else { 0 })
}

fn exec_df() -> String {
    if let Some(info) = crate::fs::get_storage_info() {
        format!("Filesystem: CottonFS\n\
                 Storage Statistics:\n\
                 +-----------------+-----------+\n\
                 | Total           | {:>9} |\n\
                 | Used            | {:>9} |\n\
                 | Free            | {:>9} |\n\
                 | Usage           | {:>8}% |\n\
                 +-----------------+-----------+\n\
                 | Files (inodes)  | {:>4}/{:<4} |\n\
                 +-----------------+-----------+",
            info.total_display(),
            info.used_display(),
            info.free_display(),
            info.usage_percent(),
            info.used_inodes,
            info.total_inodes)
    } else {
        String::from("Filesystem: RAM only (no persistent storage)\nNo disk statistics available.")
    }
}

fn exec_sync() -> String {
    crate::fs::sync_all();
    String::from("Filesystem synced to disk.")
}

fn exec_ps() -> String {
    let (queued, running, _ticks) = crate::proc::scheduler::stats();
    format!("Process List:\n  PID  STATE      NAME\n  ---  -----      ----\n  0    Running    kernel\n\nTotal: {} queued, {} running", queued, running)
}

fn exec_uptime() -> String {
    let ticks = crate::proc::scheduler::ticks();
    let seconds = ticks / 1000;
    let minutes = seconds / 60;
    let hours = minutes / 60;
    format!("Uptime: {}h {}m {}s ({} ticks)", hours, minutes % 60, seconds % 60, ticks)
}

fn exec_ls(args: &[&str]) -> String {
    let path = if args.is_empty() {
        get_cwd()
    } else {
        resolve_path(args[0])
    };
    
    match crate::fs::readdir(&path) {
        Ok(entries) => {
            if entries.is_empty() {
                String::from("(empty directory)")
            } else {
                let mut result = String::new();
                for entry in entries {
                    let type_char = match entry.file_type {
                        crate::fs::FileType::Directory => 'd',
                        crate::fs::FileType::Regular => '-',
                        crate::fs::FileType::Symlink => 'l',
                        crate::fs::FileType::CharDevice => 'c',
                        crate::fs::FileType::BlockDevice => 'b',
                        _ => '?',
                    };
                    
                    let full_path = if path == "/" {
                        format!("/{}", entry.name)
                    } else {
                        format!("{}/{}", path, entry.name)
                    };
                    
                    let size = match crate::fs::stat(&full_path) {
                        Ok(stat) => stat.size,
                        Err(_) => 0,
                    };
                    
                    result.push_str(&format!("{} {:>8} {}\n", type_char, size, entry.name));
                }
                result
            }
        }
        Err(e) => format!("ls: {}: {}", path, e),
    }
}

fn exec_cd(args: &[&str]) -> String {
    if args.is_empty() {
        set_cwd(String::from("/"));
        return String::new();
    }
    
    let path = resolve_path(args[0]);
    
    match crate::fs::lookup(&path) {
        Ok(inode) => {
            if inode.file_type() == crate::fs::FileType::Directory {
                let normalized = normalize_path(&path);
                set_cwd(normalized);
                String::new()
            } else {
                format!("cd: {}: Not a directory", args[0])
            }
        }
        Err(e) => format!("cd: {}: {}", args[0], e),
    }
}

fn exec_cat(args: &[&str]) -> String {
    if args.is_empty() {
        return String::from("cat: missing file argument");
    }
    
    let path = resolve_path(args[0]);
    
    match crate::fs::lookup(&path) {
        Ok(inode) => {
            if inode.file_type() != crate::fs::FileType::Regular {
                return format!("cat: {}: Not a regular file", args[0]);
            }
            
            let mut result = String::new();
            let mut buf = [0u8; 256];
            let mut offset = 0u64;
            
            loop {
                match inode.read(offset, &mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        for &byte in &buf[..n] {
                            if byte >= 0x20 && byte <= 0x7E || byte == b'\n' || byte == b'\r' || byte == b'\t' {
                                result.push(byte as char);
                            }
                        }
                        offset += n as u64;
                    }
                    Err(e) => {
                        result.push_str(&format!("\ncat: read error: {}", e));
                        break;
                    }
                }
            }
            result
        }
        Err(e) => format!("cat: {}: {}", args[0], e),
    }
}

fn exec_touch(args: &[&str]) -> String {
    if args.is_empty() {
        return String::from("touch: missing file argument");
    }
    
    let path = resolve_path(args[0]);
    
    if crate::fs::lookup(&path).is_ok() {
        return String::new(); // File exists, touch does nothing
    }
    
    match crate::fs::create(&path) {
        Ok(_) => format!("Created: {}", path),
        Err(e) => format!("touch: {}: {}", args[0], e),
    }
}

fn exec_mkdir(args: &[&str]) -> String {
    if args.is_empty() {
        return String::from("mkdir: missing directory argument");
    }
    
    let path = resolve_path(args[0]);
    
    match crate::fs::mkdir(&path) {
        Ok(_) => format!("Created directory: {}", path),
        Err(e) => format!("mkdir: {}: {}", args[0], e),
    }
}

fn exec_rm(args: &[&str]) -> String {
    if args.is_empty() {
        return String::from("rm: missing file argument");
    }
    
    let path = resolve_path(args[0]);
    
    match crate::fs::remove(&path) {
        Ok(_) => format!("Removed: {}", path),
        Err(e) => format!("rm: {}: {}", args[0], e),
    }
}

fn exec_write(args: &[&str]) -> String {
    if args.len() < 2 {
        return String::from("write: usage: write <file> <text>");
    }
    
    let path = resolve_path(args[0]);
    let text = args[1..].join(" ");
    
    let inode = match crate::fs::lookup(&path) {
        Ok(i) => i,
        Err(_) => {
            match crate::fs::create(&path) {
                Ok(i) => i,
                Err(e) => return format!("write: cannot create {}: {}", args[0], e),
            }
        }
    };
    
    match inode.write(0, text.as_bytes()) {
        Ok(n) => format!("Wrote {} bytes to {}", n, path),
        Err(e) => format!("write: {}: {}", args[0], e),
    }
}

/// Run the kernel shell
pub fn run() -> ! {
    set_cwd(String::from("/"));
    
    // Check for disk and auto-load on startup
    init_disk();
    
    kprintln!("");
    kprintln!("+-------------------------------------------+");
    kprintln!("|     Welcome to CottonOS Shell v0.1.0      |");
    kprintln!("|       Type 'help' for commands            |");
    kprintln!("+-------------------------------------------+");
    kprintln!("");
    
    let mut input = String::new();
    
    loop {
        kprint!("cotton:{}> ", get_cwd());
        
        // Read input
        input.clear();
        read_line(&mut input);
        
        let line = input.trim();
        if line.is_empty() {
            continue;
        }
        
        // Parse command
        let parts: Vec<&str> = line.split_whitespace().collect();
        let cmd = parts[0];
        let args = &parts[1..];
        
        // Execute command
        match cmd {
            "help" => {
                if args.is_empty() {
                    cmd_help();
                } else {
                    cmd_help_detail(args[0]);
                }
            }
            "clear" => cmd_clear(),
            "info" => cmd_info(),
            "mem" => cmd_mem(),
            "df" => cmd_df(),
            "sync" => cmd_sync(),
            "ps" => cmd_ps(),
            "uptime" => cmd_uptime(),
            "echo" => cmd_echo(args),
            "net" => cmd_net(),
            "netstats" => cmd_netstats(),
            "arptable" => cmd_arptable(),
            "arp" => cmd_arp(args),
            "ping" => cmd_ping(args),
            "dhcp" => cmd_dhcp(),
            "dns" => cmd_dns(args),
            "setip" => cmd_setip(args),
            "setmask" => cmd_setmask(args),
            "setgw" => cmd_setgw(args),
            "setdns" => cmd_setdns(args),
            "tcpconnect" => cmd_tcpconnect(args),
            "tcpsend" => cmd_tcpsend(args),
            "tcprecv" => cmd_tcprecv(),
            "tcpclose" => cmd_tcpclose(),
            "httpget" => cmd_httpget(args),
            "httpsget" => cmd_httpsget(args),
            "udpsend" => cmd_udpsend(args),
            "udprecv" => cmd_udprecv(),
            "panic" => cmd_panic(),
            "reboot" => cmd_reboot(),
            "halt" => cmd_halt(),
            // File commands
            "ls" => cmd_ls(args),
            "cd" => cmd_cd(args),
            "pwd" => cmd_pwd(),
            "cat" => cmd_cat(args),
            "touch" => cmd_touch(args),
            "mkdir" => cmd_mkdir(args),
            "rm" => cmd_rm(args),
            "write" => cmd_write(args),
            _ => kprintln!("Unknown command: '{}'. Type 'help'.", cmd),
        }
    }
}

/// Read a line from keyboard input
fn read_line(buf: &mut String) {
    loop {
        // Wait for key
        while !crate::drivers::keyboard::has_key() {
            crate::drivers::network::poll();
            crate::arch::halt();
        }
        
        // Use get_char which skips non-printable events like key releases
        if let Some(c) = crate::drivers::keyboard::get_char() {
            match c {
                '\n' | '\r' => {
                    kprintln!("");
                    return;
                }
                '\x08' | '\x7F' => {
                    // Backspace - remove last char and update display
                    if !buf.is_empty() {
                        buf.pop();
                        // Move cursor back, print space, move cursor back
                        kprint!("{}", '\x08');
                        kprint!(" ");
                        kprint!("{}", '\x08');
                    }
                }
                c if c >= ' ' && c <= '~' => {
                    // Only accept printable ASCII
                    buf.push(c);
                    kprint!("{}", c);
                }
                _ => {}
            }
        }
    }
}

fn cmd_help() {
    kprintln!("Commands: help, clear, info, mem, df, ps, uptime, echo, sync, reboot, halt");
    kprintln!("Network:  net, netstats, arptable, arp, ping, dhcp, dns, setip, setmask, setgw, setdns");
    kprintln!("TCP:      tcpconnect, tcpsend, tcprecv, tcpclose, httpget, httpsget");
    kprintln!("UDP:      udpsend, udprecv");
    kprintln!("Files:    ls, cd, pwd, cat, touch, mkdir, rm, write");
    kprintln!("");
    kprintln!("Files are stored persistently on disk (CottonFS).");
}

fn cmd_help_detail(cmd: &str) {
    match cmd {
        "ls" => kprintln!("ls [path] - List directory contents"),
        "cd" => kprintln!("cd <path> - Change directory"),
        "pwd" => kprintln!("pwd - Print working directory"),
        "cat" => kprintln!("cat <file> - Display file contents"),
        "touch" => kprintln!("touch <file> - Create empty file"),
        "mkdir" => kprintln!("mkdir <dir> - Create directory"),
        "rm" => kprintln!("rm <file> - Remove file or empty directory"),
        "write" => kprintln!("write <file> <text> - Write text to file"),
        "df" => kprintln!("df - Show disk space usage (CottonFS)"),
        "sync" => kprintln!("sync - Force write all files to disk"),
        "info" => kprintln!("info - Show system information"),
        "mem" => kprintln!("mem - Show memory statistics"),
        "ps" => kprintln!("ps - List running processes"),
        "uptime" => kprintln!("uptime - Show system uptime"),
        "echo" => kprintln!("echo <text> - Print text"),
        "net" => kprintln!("net - Show network interface information"),
        "netstats" => kprintln!("netstats - Show network packet counters"),
        "arptable" => kprintln!("arptable - Show ARP cache"),
        "arp" => kprintln!("arp <ip> - Send ARP request to host"),
        "ping" => kprintln!("ping <ip> - Send ICMP echo request"),
        "dhcp" => kprintln!("dhcp - Request IPv4 config via DHCP"),
        "dns" => kprintln!("dns <host> - Resolve hostname to IPv4"),
        "setip" => kprintln!("setip <ip> - Set interface IPv4 address"),
        "setmask" => kprintln!("setmask <mask> - Set interface netmask"),
        "setgw" => kprintln!("setgw <ip> - Set default gateway"),
        "setdns" => kprintln!("setdns <ip> - Set DNS server"),
        "tcpconnect" => kprintln!("tcpconnect <ip> <port> - Open TCP connection"),
        "tcpsend" => kprintln!("tcpsend <text> - Send TCP payload"),
        "tcprecv" => kprintln!("tcprecv - Read buffered TCP payload"),
        "tcpclose" => kprintln!("tcpclose - Close active TCP connection"),
        "httpget" => kprintln!("httpget <host-or-ip> [path] - Basic HTTP GET (no HTTPS)"),
        "httpsget" => kprintln!("httpsget <host-or-ip> [path] - HTTPS GET over TLS"),
        "udpsend" => kprintln!("udpsend <ip> <src_port> <dst_port> <text> - Send UDP datagram"),
        "udprecv" => kprintln!("udprecv - Receive one UDP datagram"),
        "clear" => kprintln!("clear - Clear the screen"),
        "reboot" => kprintln!("reboot - Restart the system"),
        "halt" => kprintln!("halt - Stop the CPU"),
        "panic" => kprintln!("panic - Trigger kernel panic (testing)"),
        _ => kprintln!("Unknown command: {}", cmd),
    }
}

fn cmd_clear() {
    // Clear screen by printing newlines or using VGA clear
    #[cfg(target_arch = "x86_64")]
    {
        let mut console = crate::drivers::console::CONSOLE.lock();
        console.clear();
    }
}

fn cmd_info() {
    kprintln!("+--------------------------------------------+");
    kprintln!("|           CottonOS System Info             |");
    kprintln!("+--------------------------------------------+");
    kprintln!("|  Kernel Version: {}                     |", crate::KERNEL_VERSION);
    kprintln!("|  Architecture:   {:?}                  |", crate::Architecture::current());
    kprintln!("|  Filesystem:     CottonFS (persistent)    |");
    kprintln!("+--------------------------------------------+");
}

fn cmd_mem() {
    let (total, used, free) = crate::mm::physical::stats();
    kprintln!("Memory Statistics:");
    kprintln!("  Total:     {} KB ({} MB)", total / 1024, total / (1024 * 1024));
    kprintln!("  Used:      {} KB ({} MB)", used / 1024, used / (1024 * 1024));
    kprintln!("  Free:      {} KB ({} MB)", free / 1024, free / (1024 * 1024));
    kprintln!("  Usage:     {}%", if total > 0 { (used * 100) / total } else { 0 });
}

fn cmd_df() {
    kprintln!("Disk Space Usage (CottonFS):");
    if let Some(info) = crate::fs::get_storage_info() {
        kprintln!("+-----------------+-----------+");
        kprintln!("| Total           | {:>9} |", info.total_display());
        kprintln!("| Used            | {:>9} |", info.used_display());
        kprintln!("| Free            | {:>9} |", info.free_display());
        kprintln!("| Usage           | {:>8}% |", info.usage_percent());
        kprintln!("+-----------------+-----------+");
        kprintln!("| Files (inodes)  | {:>4}/{:<4} |", info.used_inodes, info.total_inodes);
        kprintln!("+-----------------+-----------+");
    } else {
        kprintln!("  RAM-only filesystem (no persistent storage)");
    }
}

fn cmd_sync() {
    crate::fs::sync_all();
}

fn cmd_ps() {
    kprintln!("Process List:");
    kprintln!("  PID  STATE      NAME");
    kprintln!("  ---  -----      ----");
    
    // Get process info
    let (queued, running, _ticks) = crate::proc::scheduler::stats();
    kprintln!("  0    Running    kernel");
    kprintln!("");
    kprintln!("Total: {} queued, {} running", queued, running);
}

fn cmd_uptime() {
    let ticks = crate::proc::scheduler::ticks();
    let seconds = ticks / 1000;
    let minutes = seconds / 60;
    let hours = minutes / 60;
    
    kprintln!("Uptime: {}h {}m {}s ({} ticks)", 
              hours, minutes % 60, seconds % 60, ticks);
}

fn cmd_echo(args: &[&str]) {
    kprintln!("{}", args.join(" "));
}

fn cmd_net() {
    kprintln!("{}", exec_net());
}

fn cmd_netstats() {
    kprintln!("{}", exec_netstats());
}

fn cmd_arptable() {
    kprintln!("{}", exec_arptable());
}

fn cmd_arp(args: &[&str]) {
    kprintln!("{}", exec_arp(args));
}

fn cmd_ping(args: &[&str]) {
    kprintln!("{}", exec_ping(args));
}

fn cmd_dhcp() {
    kprintln!("{}", exec_dhcp());
}

fn cmd_dns(args: &[&str]) {
    kprintln!("{}", exec_dns(args));
}

fn cmd_setip(args: &[&str]) {
    kprintln!("{}", exec_setip(args));
}

fn cmd_setmask(args: &[&str]) {
    kprintln!("{}", exec_setmask(args));
}

fn cmd_setgw(args: &[&str]) {
    kprintln!("{}", exec_setgw(args));
}

fn cmd_setdns(args: &[&str]) {
    kprintln!("{}", exec_setdns(args));
}

fn cmd_tcpconnect(args: &[&str]) {
    kprintln!("{}", exec_tcpconnect(args));
}

fn cmd_tcpsend(args: &[&str]) {
    kprintln!("{}", exec_tcpsend(args));
}

fn cmd_tcprecv() {
    kprintln!("{}", exec_tcprecv());
}

fn cmd_tcpclose() {
    kprintln!("{}", exec_tcpclose());
}

fn cmd_httpget(args: &[&str]) {
    kprintln!("{}", exec_httpget(args));
}

fn cmd_httpsget(args: &[&str]) {
    kprintln!("{}", exec_httpsget(args));
}

fn cmd_udpsend(args: &[&str]) {
    kprintln!("{}", exec_udpsend(args));
}

fn cmd_udprecv() {
    kprintln!("{}", exec_udprecv());
}

fn cmd_panic() {
    panic!("User-triggered panic via shell command");
}

fn cmd_reboot() {
    kprintln!("Rebooting...");
    #[cfg(target_arch = "x86_64")]
    unsafe {
        // Try keyboard controller reset
        let mut good = false;
        for _ in 0..1000 {
            if crate::arch::x86_64::inb(0x64) & 0x02 == 0 {
                good = true;
                break;
            }
        }
        if good {
            crate::arch::x86_64::outb(0x64, 0xFE);
        }
        
        // If that fails, triple fault
        crate::arch::disable_interrupts();
        core::arch::asm!("lidt [{}]", in(reg) &[0u64; 2], options(nostack));
        core::arch::asm!("int3", options(nostack));
    }
    loop { crate::arch::halt(); }
}

fn cmd_halt() {
    kprintln!("System halted.");
    crate::arch::disable_interrupts();
    loop {
        crate::arch::halt();
    }
}
// ==================== FILE COMMANDS ====================

fn cmd_ls(args: &[&str]) {
    let path = if args.is_empty() {
        get_cwd()
    } else {
        resolve_path(args[0])
    };
    
    match crate::fs::readdir(&path) {
        Ok(entries) => {
            if entries.is_empty() {
                kprintln!("(empty directory)");
            } else {
                for entry in entries {
                    let type_char = match entry.file_type {
                        crate::fs::FileType::Directory => 'd',
                        crate::fs::FileType::Regular => '-',
                        crate::fs::FileType::Symlink => 'l',
                        crate::fs::FileType::CharDevice => 'c',
                        crate::fs::FileType::BlockDevice => 'b',
                        _ => '?',
                    };
                    
                    // Try to get file size
                    let full_path = if path == "/" {
                        format!("/{}", entry.name)
                    } else {
                        format!("{}/{}", path, entry.name)
                    };
                    
                    let size = match crate::fs::stat(&full_path) {
                        Ok(stat) => stat.size,
                        Err(_) => 0,
                    };
                    
                    kprintln!("{} {:>8} {}", type_char, size, entry.name);
                }
            }
        }
        Err(e) => kprintln!("ls: {}: {}", path, e),
    }
}

fn cmd_cd(args: &[&str]) {
    if args.is_empty() {
        set_cwd(String::from("/"));
        return;
    }
    
    let path = resolve_path(args[0]);
    
    // Verify it's a directory
    match crate::fs::lookup(&path) {
        Ok(inode) => {
            if inode.file_type() == crate::fs::FileType::Directory {
                // Normalize the path
                let normalized = normalize_path(&path);
                set_cwd(normalized);
            } else {
                kprintln!("cd: {}: Not a directory", args[0]);
            }
        }
        Err(e) => kprintln!("cd: {}: {}", args[0], e),
    }
}

fn normalize_path(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => { parts.pop(); }
            p => parts.push(p),
        }
    }
    
    if parts.is_empty() {
        String::from("/")
    } else {
        format!("/{}", parts.join("/"))
    }
}

fn cmd_pwd() {
    kprintln!("{}", get_cwd());
}

fn cmd_cat(args: &[&str]) {
    if args.is_empty() {
        kprintln!("cat: missing file argument");
        return;
    }
    
    let path = resolve_path(args[0]);
    
    match crate::fs::lookup(&path) {
        Ok(inode) => {
            if inode.file_type() != crate::fs::FileType::Regular {
                kprintln!("cat: {}: Not a regular file", args[0]);
                return;
            }
            
            let mut buf = [0u8; 256];
            let mut offset = 0u64;
            
            loop {
                match inode.read(offset, &mut buf) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        for &byte in &buf[..n] {
                            if byte >= 0x20 && byte <= 0x7E || byte == b'\n' || byte == b'\r' || byte == b'\t' {
                                kprint!("{}", byte as char);
                            }
                        }
                        offset += n as u64;
                    }
                    Err(e) => {
                        kprintln!("cat: read error: {}", e);
                        break;
                    }
                }
            }
            kprintln!(""); // Ensure newline at end
        }
        Err(e) => kprintln!("cat: {}: {}", args[0], e),
    }
}

fn cmd_touch(args: &[&str]) {
    if args.is_empty() {
        kprintln!("touch: missing file argument");
        return;
    }
    
    let path = resolve_path(args[0]);
    
    // Check if file already exists
    if crate::fs::lookup(&path).is_ok() {
        // File exists, do nothing (touch behavior)
        return;
    }
    
    match crate::fs::create(&path) {
        Ok(_) => kprintln!("Created: {}", path),
        Err(e) => kprintln!("touch: {}: {}", args[0], e),
    }
}

fn cmd_mkdir(args: &[&str]) {
    if args.is_empty() {
        kprintln!("mkdir: missing directory argument");
        return;
    }
    
    let path = resolve_path(args[0]);
    
    match crate::fs::mkdir(&path) {
        Ok(_) => kprintln!("Created directory: {}", path),
        Err(e) => kprintln!("mkdir: {}: {}", args[0], e),
    }
}

fn cmd_rm(args: &[&str]) {
    if args.is_empty() {
        kprintln!("rm: missing file argument");
        return;
    }
    
    let path = resolve_path(args[0]);
    
    match crate::fs::remove(&path) {
        Ok(_) => kprintln!("Removed: {}", path),
        Err(e) => kprintln!("rm: {}: {}", args[0], e),
    }
}

fn cmd_write(args: &[&str]) {
    if args.len() < 2 {
        kprintln!("write: usage: write <file> <text>");
        return;
    }
    
    let path = resolve_path(args[0]);
    let text = args[1..].join(" ");
    
    // Create file if it doesn't exist
    let inode = match crate::fs::lookup(&path) {
        Ok(i) => i,
        Err(_) => {
            match crate::fs::create(&path) {
                Ok(i) => i,
                Err(e) => {
                    kprintln!("write: cannot create {}: {}", args[0], e);
                    return;
                }
            }
        }
    };
    
    // Write text
    match inode.write(0, text.as_bytes()) {
        Ok(n) => kprintln!("Wrote {} bytes to {}", n, path),
        Err(e) => kprintln!("write: {}: {}", args[0], e),
    }
}

// ==================== DISK FUNCTIONS ====================

const DISK_MAGIC: &[u8; 8] = b"COTTONFS";

/// Initialize disk - check availability (no auto-load, too slow)
fn init_disk() {
    #[cfg(target_arch = "x86_64")]
    {
        use crate::drivers::storage::ata::AtaDevice;
        
        if AtaDevice::detect(0, 0).is_some() {
            set_has_disk(true);
        } else {
            set_has_disk(false);
        }
    }
    
    #[cfg(not(target_arch = "x86_64"))]
    {
        set_has_disk(false);
    }
}