//! # STUN 客户端
//!
//! 简单 STUN 实现，用于获取公网 IP 和端口映射。
//!
//! 支持的 STUN 属性：
//! - MAPPED-ADDRESS: 公网地址映射
//! - RESPONSE-ADDRESS: 响应地址
//! - CHANGE-REQUEST: 改变请求（用于 NAT 类型检测）
//!
//! STUN 消息类型：
//! - Binding Request: 0x0001
//! - Binding Response: 0x0101

use crate::nat::{NatInfo, NatType};
use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

/// STUN 服务器列表（公共服务器）
const STUN_SERVERS: &[(&str, u16)] = &[
    ("stun.l.google.com", 19302),
    ("stun1.l.google.com", 19302),
    ("stun.cloudflare.com", 3478),
];

/// STUN 消息类型
const _STUN_BINDING_REQUEST: u16 = 0x0001;
const STUN_BINDING_RESPONSE: u16 = 0x0101;

/// STUN 属性类型
const STUN_ATTR_MAPPED_ADDRESS: u16 = 0x0001;
const STUN_ATTR_CHANGE_REQUEST: u16 = 0x0003;
const STUN_ATTR_XOR_MAPPED_ADDRESS: u16 = 0x0020;

/// STUN 客户端
pub struct StunClient {
    servers: Vec<(String, u16)>,
    timeout_ms: u64,
}

impl StunClient {
    /// 创建 STUN 客户端
    pub fn new() -> Self {
        Self {
            servers: STUN_SERVERS.iter().map(|(h, p)| (h.to_string(), *p)).collect(),
            timeout_ms: 3000,
        }
    }

    /// 创建自定义服务器的 STUN 客户端
    pub fn with_servers(servers: Vec<(String, u16)>) -> Self {
        Self {
            servers,
            timeout_ms: 3000,
        }
    }

    /// 获取公网 IP（快速查询）
    pub fn get_public_address(&self) -> Option<SocketAddr> {
        for (host, port) in &self.servers {
            if let Ok(addr) = self.query_server(host, *port) {
                return Some(addr);
            }
        }
        None
    }

    /// 检测 NAT 类型（同步版本）
    pub fn detect_nat_type(&self, local_socket: &UdpSocket) -> NatInfo {
        let local_addr = match local_socket.local_addr() {
            Ok(addr) => addr,
            Err(_) => {
                return NatInfo::unknown("0.0.0.0", 0);
            }
        };

        // 尝试每个 STUN 服务器
        for (host, port) in &self.servers {
            if let Some(nat_info) = self.detect_with_server(local_socket, host, *port, local_addr) {
                return nat_info;
            }
        }

        // 所有服务器都失败，保守返回未知
        NatInfo::unknown(&local_addr.ip().to_string(), local_addr.port())
    }

    /// 使用指定服务器检测 NAT
    fn detect_with_server(
        &self,
        local_socket: &UdpSocket,
        host: &str,
        port: u16,
        local_addr: SocketAddr,
    ) -> Option<NatInfo> {
        let server_addr: SocketAddr = format!("{}:{}", host, port).parse().ok()?;

        // 1. 发送普通 Binding Request
        let request = build_binding_request(false, false);
        if local_socket.send_to(&request, server_addr).is_err() {
            return None;
        }

        // 2. 等待响应（带超时）
        let mut buf = [0u8; 1024];
        local_socket
            .set_read_timeout(Some(Duration::from_millis(self.timeout_ms)))
            .ok()?;

        let result = local_socket.recv_from(&mut buf);

        let response = match result {
            Ok((len, _)) => &buf[..len],
            Err(_) => return None,
        };

        // 3. 解析 MAPPED-ADDRESS
        let mapped = parse_mapped_address(response)?;

        // 4. 判断 NAT 类型
        let nat_type = if mapped == local_addr {
            NatType::Open
        } else {
            // 简单判断：如果公网端口与本地端口不同，很可能是对称型
            if mapped.port() != local_addr.port() || mapped.ip() != local_addr.ip() {
                // 尝试发送带 change-request 的请求来进一步判断
                NatType::Symmetric
            } else {
                NatType::FullCone
            }
        };

        Some(NatInfo {
            nat_type,
            local_ip: local_addr.ip().to_string(),
            local_port: local_addr.port(),
            public_ip: Some(mapped.ip().to_string()),
            public_port: Some(mapped.port()),
            is_complete: true,
        })
    }

    /// 查询单个服务器
    fn query_server(&self, host: &str, port: u16) -> Result<SocketAddr, std::io::Error> {
        let server_addr: SocketAddr = format!("{}:{}", host, port)
            .parse()
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "Invalid server address"))?;

        // 创建 UDP socket
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_read_timeout(Some(Duration::from_millis(self.timeout_ms)))?;

        // 发送 Binding Request
        let request = build_binding_request(false, false);
        socket.send_to(&request, server_addr)?;

        // 接收响应
        let mut buf = [0u8; 1024];
        let (len, _) = socket.recv_from(&mut buf)?;

        // 解析 MAPPED-ADDRESS
        parse_mapped_address(&buf[..len])
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "No MAPPED-ADDRESS"))
    }
}

/// 构建 STUN Binding Request
fn build_binding_request(change_ip: bool, change_port: bool) -> Vec<u8> {
    let mut msg = Vec::with_capacity(20 + 8);

    // Message Type: Binding Request (0x0001)
    msg.push(0x00);
    msg.push(0x01);

    // Message Length (不包括 20 字节 header)
    msg.push(0x00);
    msg.push(0x08); // CHANGE-REQUEST 是 8 字节

    // Magic Cookie
    msg.extend_from_slice(&[0x21, 0x12, 0xA4, 0x42]);

    // Transaction ID (96 bits = 12 bytes)
    let tid: [u8; 12] = rand::random();
    msg.extend_from_slice(&tid);

    // CHANGE-REQUEST Attribute
    let mut change_bits: u32 = 0;
    if change_ip {
        change_bits |= 0x04;
    }
    if change_port {
        change_bits |= 0x02;
    }

    msg.extend_from_slice(&(STUN_ATTR_CHANGE_REQUEST).to_be_bytes()); // Type
    msg.extend_from_slice(&8u16.to_be_bytes()); // Length
    msg.extend_from_slice(&change_bits.to_be_bytes());
    msg.extend_from_slice(&[0u8; 4]); // 填充到 8 字节

    msg
}

/// 解析 MAPPED-ADDRESS 属性
fn parse_mapped_address(data: &[u8]) -> Option<SocketAddr> {
    if data.len() < 20 {
        return None;
    }

    // 检查消息类型是否为 Binding Response
    let msg_type = u16::from_be_bytes([data[0], data[1]]);
    if msg_type != STUN_BINDING_RESPONSE {
        return None;
    }

    // 跳过 20 字节 header
    let mut pos = 20;

    while pos + 4 < data.len() {
        let attr_type = u16::from_be_bytes([data[pos], data[pos + 1]]);
        let attr_len = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;

        if attr_type == STUN_ATTR_MAPPED_ADDRESS || attr_type == STUN_ATTR_XOR_MAPPED_ADDRESS {
            if pos + 4 + attr_len > data.len() {
                return None;
            }

            // 跳过 family (1 byte) 和 port (2 bytes)
            let port = if attr_type == STUN_ATTR_XOR_MAPPED_ADDRESS {
                let xor_port = u16::from_be_bytes([data[pos + 6], data[pos + 7]]);
                xor_port ^ 0x2112 // XOR with magic cookie high bits
            } else {
                u16::from_be_bytes([data[pos + 6], data[pos + 7]])
            };

            let ip = if attr_type == STUN_ATTR_XOR_MAPPED_ADDRESS {
                let xored = u32::from_be_bytes([data[pos + 8], data[pos + 9], data[pos + 10], data[pos + 11]]);
                let magic = u32::from_be_bytes([0x21, 0x12, 0xA4, 0x42]);
                std::net::Ipv4Addr::from(xored ^ magic)
            } else {
                std::net::Ipv4Addr::from([data[pos + 8], data[pos + 9], data[pos + 10], data[pos + 11]])
            };

            return SocketAddr::from((ip, port)).into();
        }

        // 移动到下一个属性（4字节头 + 长度，4字节对齐）
        pos += 4 + ((attr_len + 3) & !3);
    }

    None
}

impl Default for StunClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_binding_request() {
        let request = build_binding_request(false, false);
        assert!(request.len() >= 28);
        assert_eq!(request[0], 0x00);
        assert_eq!(request[1], 0x01);
    }

    #[test]
    fn test_stun_client_creation() {
        let client = StunClient::new();
        assert!(!client.servers.is_empty());
    }
}
