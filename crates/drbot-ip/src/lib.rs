//! IP address utilities for drbot.
//!
//! This crate provides:
//! - IP address parsing
//! - IP range checking
//! - CIDR notation support

use serde::{Deserialize, Serialize};
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::str::FromStr;
use thiserror::Error;

/// IP error types.
#[derive(Error, Debug)]
pub enum IpError {
    #[error("Invalid IP address: {0}")]
    InvalidAddress(String),

    #[error("Invalid CIDR: {0}")]
    InvalidCidr(String),

    #[error("Invalid range: {0}")]
    InvalidRange(String),
}

/// Result type for IP operations.
pub type Result<T> = std::result::Result<T, IpError>;

/// IPv4 CIDR range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ipv4Cidr {
    /// Network address.
    pub network: Ipv4Addr,
    /// Prefix length (0-32).
    pub prefix: u8,
}

impl Ipv4Cidr {
    /// Create new CIDR range.
    pub fn new(network: Ipv4Addr, prefix: u8) -> Result<Self> {
        if prefix > 32 {
            return Err(IpError::InvalidCidr(format!(
                "Prefix {} exceeds 32",
                prefix
            )));
        }

        // Normalize network address
        let mask = Self::prefix_to_mask(prefix);
        let network_bits = u32::from(network) & mask;
        let network = Ipv4Addr::from(network_bits);

        Ok(Self { network, prefix })
    }

    /// Parse CIDR from string.
    pub fn parse(s: &str) -> Result<Self> {
        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() != 2 {
            return Err(IpError::InvalidCidr(s.to_string()));
        }

        let network = parts[0]
            .parse()
            .map_err(|_| IpError::InvalidAddress(parts[0].to_string()))?;
        let prefix = parts[1]
            .parse()
            .map_err(|_| IpError::InvalidCidr(s.to_string()))?;

        Self::new(network, prefix)
    }

    /// Check if address is in range.
    pub fn contains(&self, addr: Ipv4Addr) -> bool {
        let mask = Self::prefix_to_mask(self.prefix);
        let network_bits = u32::from(self.network);
        let addr_bits = u32::from(addr);
        (addr_bits & mask) == network_bits
    }

    /// Get network mask.
    pub fn mask(&self) -> Ipv4Addr {
        Ipv4Addr::from(Self::prefix_to_mask(self.prefix))
    }

    /// Get broadcast address.
    pub fn broadcast(&self) -> Ipv4Addr {
        let mask = Self::prefix_to_mask(self.prefix);
        let network_bits = u32::from(self.network);
        let broadcast_bits = network_bits | !mask;
        Ipv4Addr::from(broadcast_bits)
    }

    /// Get first usable address.
    pub fn first_address(&self) -> Ipv4Addr {
        if self.prefix >= 31 {
            self.network
        } else {
            Ipv4Addr::from(u32::from(self.network) + 1)
        }
    }

    /// Get last usable address.
    pub fn last_address(&self) -> Ipv4Addr {
        if self.prefix >= 31 {
            self.broadcast()
        } else {
            Ipv4Addr::from(u32::from(self.broadcast()) - 1)
        }
    }

    /// Get number of addresses in range.
    pub fn size(&self) -> u64 {
        1u64 << (32 - self.prefix as u32)
    }

    fn prefix_to_mask(prefix: u8) -> u32 {
        if prefix == 0 {
            0
        } else {
            !0u32 << (32 - prefix)
        }
    }
}

impl FromStr for Ipv4Cidr {
    type Err = IpError;

    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

impl fmt::Display for Ipv4Cidr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.network, self.prefix)
    }
}

/// IPv6 CIDR range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ipv6Cidr {
    /// Network address.
    pub network: Ipv6Addr,
    /// Prefix length (0-128).
    pub prefix: u8,
}

impl Ipv6Cidr {
    /// Create new CIDR range.
    pub fn new(network: Ipv6Addr, prefix: u8) -> Result<Self> {
        if prefix > 128 {
            return Err(IpError::InvalidCidr(format!(
                "Prefix {} exceeds 128",
                prefix
            )));
        }

        // Normalize network address
        let mask = Self::prefix_to_mask(prefix);
        let network_bits = u128::from(network) & mask;
        let network = Ipv6Addr::from(network_bits);

        Ok(Self { network, prefix })
    }

    /// Parse CIDR from string.
    pub fn parse(s: &str) -> Result<Self> {
        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() != 2 {
            return Err(IpError::InvalidCidr(s.to_string()));
        }

        let network = parts[0]
            .parse()
            .map_err(|_| IpError::InvalidAddress(parts[0].to_string()))?;
        let prefix = parts[1]
            .parse()
            .map_err(|_| IpError::InvalidCidr(s.to_string()))?;

        Self::new(network, prefix)
    }

    /// Check if address is in range.
    pub fn contains(&self, addr: Ipv6Addr) -> bool {
        let mask = Self::prefix_to_mask(self.prefix);
        let network_bits = u128::from(self.network);
        let addr_bits = u128::from(addr);
        (addr_bits & mask) == network_bits
    }

    fn prefix_to_mask(prefix: u8) -> u128 {
        if prefix == 0 {
            0
        } else {
            !0u128 << (128 - prefix)
        }
    }
}

impl FromStr for Ipv6Cidr {
    type Err = IpError;

    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

impl fmt::Display for Ipv6Cidr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.network, self.prefix)
    }
}

/// IP utilities.
pub struct Ip;

impl Ip {
    /// Parse IP address.
    pub fn parse(s: &str) -> Result<IpAddr> {
        s.parse()
            .map_err(|_| IpError::InvalidAddress(s.to_string()))
    }

    /// Parse IPv4 address.
    pub fn parse_v4(s: &str) -> Result<Ipv4Addr> {
        s.parse()
            .map_err(|_| IpError::InvalidAddress(s.to_string()))
    }

    /// Parse IPv6 address.
    pub fn parse_v6(s: &str) -> Result<Ipv6Addr> {
        s.parse()
            .map_err(|_| IpError::InvalidAddress(s.to_string()))
    }

    /// Check if address is private.
    pub fn is_private(addr: IpAddr) -> bool {
        match addr {
            IpAddr::V4(v4) => Self::is_private_v4(v4),
            IpAddr::V6(v6) => Self::is_private_v6(v6),
        }
    }

    /// Check if IPv4 address is private.
    pub fn is_private_v4(addr: Ipv4Addr) -> bool {
        let octets = addr.octets();
        // 10.0.0.0/8
        octets[0] == 10
        // 172.16.0.0/12
        || (octets[0] == 172 && (16..=31).contains(&octets[1]))
        // 192.168.0.0/16
        || (octets[0] == 192 && octets[1] == 168)
    }

    /// Check if IPv6 address is private (unique local).
    pub fn is_private_v6(addr: Ipv6Addr) -> bool {
        let segments = addr.segments();
        // fc00::/7 (Unique Local)
        (segments[0] & 0xfe00) == 0xfc00
    }

    /// Check if address is loopback.
    pub fn is_loopback(addr: IpAddr) -> bool {
        addr.is_loopback()
    }

    /// Check if address is multicast.
    pub fn is_multicast(addr: IpAddr) -> bool {
        addr.is_multicast()
    }

    /// Check if address is link-local.
    pub fn is_link_local(addr: IpAddr) -> bool {
        match addr {
            IpAddr::V4(v4) => {
                let octets = v4.octets();
                octets[0] == 169 && octets[1] == 254
            }
            IpAddr::V6(v6) => {
                let segments = v6.segments();
                (segments[0] & 0xffc0) == 0xfe80
            }
        }
    }

    /// Convert IPv4 to IPv6-mapped address.
    pub fn v4_to_v6_mapped(addr: Ipv4Addr) -> Ipv6Addr {
        addr.to_ipv6_mapped()
    }

    /// Convert IPv6-mapped address to IPv4.
    pub fn v6_to_v4_mapped(addr: Ipv6Addr) -> Option<Ipv4Addr> {
        addr.to_ipv4_mapped()
    }
}

/// Well-known IP ranges.
pub struct WellKnown;

impl WellKnown {
    /// Private networks (RFC 1918).
    pub fn private_networks() -> Vec<Ipv4Cidr> {
        vec![
            Ipv4Cidr::parse("10.0.0.0/8").unwrap(),
            Ipv4Cidr::parse("172.16.0.0/12").unwrap(),
            Ipv4Cidr::parse("192.168.0.0/16").unwrap(),
        ]
    }

    /// Loopback network.
    pub fn loopback() -> Ipv4Cidr {
        Ipv4Cidr::parse("127.0.0.0/8").unwrap()
    }

    /// Link-local network.
    pub fn link_local() -> Ipv4Cidr {
        Ipv4Cidr::parse("169.254.0.0/16").unwrap()
    }

    /// Multicast.
    pub fn multicast() -> Ipv4Cidr {
        Ipv4Cidr::parse("224.0.0.0/4").unwrap()
    }

    /// Check if address is in any private network.
    pub fn is_private(addr: Ipv4Addr) -> bool {
        Self::private_networks()
            .iter()
            .any(|cidr| cidr.contains(addr))
    }
}

/// IP range (start to end).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ipv4Range {
    /// Start address (inclusive).
    pub start: Ipv4Addr,
    /// End address (inclusive).
    pub end: Ipv4Addr,
}

impl Ipv4Range {
    /// Create new IP range.
    pub fn new(start: Ipv4Addr, end: Ipv4Addr) -> Result<Self> {
        if u32::from(start) > u32::from(end) {
            return Err(IpError::InvalidRange(format!(
                "Start {} is greater than end {}",
                start, end
            )));
        }
        Ok(Self { start, end })
    }

    /// Parse range from string (e.g., "192.168.1.1-192.168.1.100").
    pub fn parse(s: &str) -> Result<Self> {
        let parts: Vec<&str> = s.split('-').collect();
        if parts.len() != 2 {
            return Err(IpError::InvalidRange(s.to_string()));
        }

        let start = parts[0]
            .trim()
            .parse()
            .map_err(|_| IpError::InvalidAddress(parts[0].to_string()))?;
        let end = parts[1]
            .trim()
            .parse()
            .map_err(|_| IpError::InvalidAddress(parts[1].to_string()))?;

        Self::new(start, end)
    }

    /// Check if address is in range.
    pub fn contains(&self, addr: Ipv4Addr) -> bool {
        let addr_u32 = u32::from(addr);
        addr_u32 >= u32::from(self.start) && addr_u32 <= u32::from(self.end)
    }

    /// Get number of addresses in range.
    pub fn size(&self) -> u64 {
        (u32::from(self.end) - u32::from(self.start)) as u64 + 1
    }

    /// Iterate over addresses in range.
    pub fn iter(&self) -> impl Iterator<Item = Ipv4Addr> {
        let start = u32::from(self.start);
        let end = u32::from(self.end);
        (start..=end).map(Ipv4Addr::from)
    }
}

impl fmt::Display for Ipv4Range {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.start, self.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cidr_parse() {
        let cidr = Ipv4Cidr::parse("192.168.1.0/24").unwrap();
        assert_eq!(cidr.network, Ipv4Addr::new(192, 168, 1, 0));
        assert_eq!(cidr.prefix, 24);
    }

    #[test]
    fn test_cidr_contains() {
        let cidr = Ipv4Cidr::parse("192.168.1.0/24").unwrap();
        assert!(cidr.contains(Ipv4Addr::new(192, 168, 1, 1)));
        assert!(cidr.contains(Ipv4Addr::new(192, 168, 1, 254)));
        assert!(!cidr.contains(Ipv4Addr::new(192, 168, 2, 1)));
    }

    #[test]
    fn test_cidr_broadcast() {
        let cidr = Ipv4Cidr::parse("192.168.1.0/24").unwrap();
        assert_eq!(cidr.broadcast(), Ipv4Addr::new(192, 168, 1, 255));
    }

    #[test]
    fn test_cidr_size() {
        let cidr = Ipv4Cidr::parse("192.168.1.0/24").unwrap();
        assert_eq!(cidr.size(), 256);

        let cidr = Ipv4Cidr::parse("10.0.0.0/8").unwrap();
        assert_eq!(cidr.size(), 16777216);
    }

    #[test]
    fn test_is_private() {
        assert!(Ip::is_private(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(Ip::is_private(IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
        assert!(Ip::is_private(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
        assert!(!Ip::is_private(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
    }

    #[test]
    fn test_is_loopback() {
        assert!(Ip::is_loopback(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
        assert!(!Ip::is_loopback(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
    }

    #[test]
    fn test_range() {
        let range = Ipv4Range::parse("192.168.1.1-192.168.1.10").unwrap();
        assert!(range.contains(Ipv4Addr::new(192, 168, 1, 5)));
        assert!(!range.contains(Ipv4Addr::new(192, 168, 1, 11)));
        assert_eq!(range.size(), 10);
    }

    #[test]
    fn test_ipv6_cidr() {
        let cidr = Ipv6Cidr::parse("2001:db8::/32").unwrap();
        assert!(cidr.contains(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)));
        assert!(!cidr.contains(Ipv6Addr::new(0x2001, 0xdb9, 0, 0, 0, 0, 0, 1)));
    }
}
