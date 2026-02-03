//! Port handling for drbot.
//!
//! This crate provides:
//! - Port validation
//! - Port availability checking
//! - Well-known port info

use std::net::{SocketAddr, TcpListener};
use thiserror::Error;
use tokio::net::TcpStream;

/// Port error types.
#[derive(Error, Debug)]
pub enum PortError {
    #[error("Invalid port: {0}")]
    InvalidPort(u16),

    #[error("Port {0} is in use")]
    InUse(u16),

    #[error("Port {0} requires elevated privileges")]
    RequiresPrivilege(u16),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type for port operations.
pub type Result<T> = std::result::Result<T, PortError>;

/// Port utilities.
pub struct Port;

impl Port {
    /// Check if port is valid.
    pub fn is_valid(port: u16) -> bool {
        port > 0
    }

    /// Check if port is privileged (< 1024).
    pub fn is_privileged(port: u16) -> bool {
        port < 1024
    }

    /// Check if port is in user range (1024-49151).
    pub fn is_user_port(port: u16) -> bool {
        (1024..=49151).contains(&port)
    }

    /// Check if port is in dynamic range (49152-65535).
    pub fn is_dynamic(port: u16) -> bool {
        port >= 49152
    }

    /// Check if port is available on localhost.
    pub fn is_available(port: u16) -> bool {
        TcpListener::bind(("127.0.0.1", port)).is_ok()
    }

    /// Check if port is available on all interfaces.
    pub fn is_available_all(port: u16) -> bool {
        TcpListener::bind(("0.0.0.0", port)).is_ok()
    }

    /// Check if port is available on specific address.
    pub fn is_available_on(addr: &str, port: u16) -> bool {
        TcpListener::bind((addr, port)).is_ok()
    }

    /// Find an available port starting from the given port.
    pub fn find_available(start: u16) -> Option<u16> {
        (start..=65535).find(|&p| Self::is_available(p))
    }

    /// Find an available port in range.
    pub fn find_available_in_range(start: u16, end: u16) -> Option<u16> {
        (start..=end).find(|&p| Self::is_available(p))
    }

    /// Get a random available port.
    pub fn random_available() -> Option<u16> {
        // Bind to port 0 to get a random available port
        let listener = TcpListener::bind("127.0.0.1:0").ok()?;
        listener.local_addr().ok().map(|addr| addr.port())
    }

    /// Get multiple random available ports.
    pub fn random_available_n(count: usize) -> Vec<u16> {
        let mut ports = Vec::with_capacity(count);
        let mut listeners = Vec::with_capacity(count);

        for _ in 0..count {
            if let Ok(listener) = TcpListener::bind("127.0.0.1:0") {
                if let Ok(addr) = listener.local_addr() {
                    ports.push(addr.port());
                    listeners.push(listener); // Keep listener alive to reserve port
                }
            }
        }

        ports
    }

    /// Get well-known port name.
    pub fn name(port: u16) -> Option<&'static str> {
        WellKnown::name(port)
    }

    /// Get well-known port number.
    pub fn number(name: &str) -> Option<u16> {
        WellKnown::port(name)
    }
}

/// Async port utilities.
pub struct AsyncPort;

impl AsyncPort {
    /// Check if port is reachable.
    pub async fn is_reachable(addr: &str, port: u16) -> bool {
        let socket_addr: SocketAddr = match format!("{}:{}", addr, port).parse() {
            Ok(addr) => addr,
            Err(_) => return false,
        };

        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            TcpStream::connect(socket_addr),
        )
        .await
        .map(|r| r.is_ok())
        .unwrap_or(false)
    }

    /// Check if localhost port is reachable.
    pub async fn is_listening(port: u16) -> bool {
        Self::is_reachable("127.0.0.1", port).await
    }

    /// Wait for port to become available.
    pub async fn wait_available(port: u16, timeout: std::time::Duration) -> bool {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if Port::is_available(port) {
                return true;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        false
    }

    /// Wait for port to become reachable.
    pub async fn wait_reachable(addr: &str, port: u16, timeout: std::time::Duration) -> bool {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if Self::is_reachable(addr, port).await {
                return true;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        false
    }

    /// Scan ports in range.
    pub async fn scan_range(addr: &str, start: u16, end: u16) -> Vec<u16> {
        let mut open_ports = Vec::new();

        for port in start..=end {
            if Self::is_reachable(addr, port).await {
                open_ports.push(port);
            }
        }

        open_ports
    }
}

/// Well-known ports.
pub struct WellKnown;

impl WellKnown {
    /// FTP data.
    pub const FTP_DATA: u16 = 20;
    /// FTP control.
    pub const FTP: u16 = 21;
    /// SSH.
    pub const SSH: u16 = 22;
    /// Telnet.
    pub const TELNET: u16 = 23;
    /// SMTP.
    pub const SMTP: u16 = 25;
    /// DNS.
    pub const DNS: u16 = 53;
    /// HTTP.
    pub const HTTP: u16 = 80;
    /// POP3.
    pub const POP3: u16 = 110;
    /// NTP.
    pub const NTP: u16 = 123;
    /// IMAP.
    pub const IMAP: u16 = 143;
    /// HTTPS.
    pub const HTTPS: u16 = 443;
    /// SMTP over SSL.
    pub const SMTPS: u16 = 465;
    /// SMTP submission.
    pub const SUBMISSION: u16 = 587;
    /// IMAPS.
    pub const IMAPS: u16 = 993;
    /// POP3S.
    pub const POP3S: u16 = 995;
    /// MySQL.
    pub const MYSQL: u16 = 3306;
    /// PostgreSQL.
    pub const POSTGRESQL: u16 = 5432;
    /// Redis.
    pub const REDIS: u16 = 6379;
    /// MongoDB.
    pub const MONGODB: u16 = 27017;

    /// Get port name.
    pub fn name(port: u16) -> Option<&'static str> {
        match port {
            20 => Some("ftp-data"),
            21 => Some("ftp"),
            22 => Some("ssh"),
            23 => Some("telnet"),
            25 => Some("smtp"),
            53 => Some("dns"),
            80 => Some("http"),
            110 => Some("pop3"),
            123 => Some("ntp"),
            143 => Some("imap"),
            443 => Some("https"),
            465 => Some("smtps"),
            587 => Some("submission"),
            993 => Some("imaps"),
            995 => Some("pop3s"),
            3306 => Some("mysql"),
            5432 => Some("postgresql"),
            6379 => Some("redis"),
            8080 => Some("http-alt"),
            8443 => Some("https-alt"),
            27017 => Some("mongodb"),
            _ => None,
        }
    }

    /// Get port number by name.
    pub fn port(name: &str) -> Option<u16> {
        match name.to_lowercase().as_str() {
            "ftp-data" => Some(20),
            "ftp" => Some(21),
            "ssh" => Some(22),
            "telnet" => Some(23),
            "smtp" => Some(25),
            "dns" => Some(53),
            "http" => Some(80),
            "pop3" => Some(110),
            "ntp" => Some(123),
            "imap" => Some(143),
            "https" => Some(443),
            "smtps" => Some(465),
            "submission" => Some(587),
            "imaps" => Some(993),
            "pop3s" => Some(995),
            "mysql" => Some(3306),
            "postgresql" | "postgres" => Some(5432),
            "redis" => Some(6379),
            "http-alt" => Some(8080),
            "https-alt" => Some(8443),
            "mongodb" | "mongo" => Some(27017),
            _ => None,
        }
    }
}

/// Port range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortRange {
    /// Start port (inclusive).
    pub start: u16,
    /// End port (inclusive).
    pub end: u16,
}

impl PortRange {
    /// Create new port range.
    pub fn new(start: u16, end: u16) -> Self {
        Self {
            start: start.min(end),
            end: start.max(end),
        }
    }

    /// Check if port is in range.
    pub fn contains(&self, port: u16) -> bool {
        port >= self.start && port <= self.end
    }

    /// Get number of ports in range.
    pub fn size(&self) -> u32 {
        (self.end - self.start) as u32 + 1
    }

    /// Iterate over ports in range.
    pub fn iter(&self) -> impl Iterator<Item = u16> {
        self.start..=self.end
    }

    /// User port range (1024-49151).
    pub fn user() -> Self {
        Self::new(1024, 49151)
    }

    /// Dynamic port range (49152-65535).
    pub fn dynamic() -> Self {
        Self::new(49152, 65535)
    }

    /// Privileged port range (1-1023).
    pub fn privileged() -> Self {
        Self::new(1, 1023)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_port_validation() {
        assert!(Port::is_valid(80));
        assert!(Port::is_valid(443));
        assert!(!Port::is_valid(0));
    }

    #[test]
    fn test_port_categories() {
        assert!(Port::is_privileged(80));
        assert!(Port::is_privileged(443));
        assert!(!Port::is_privileged(8080));

        assert!(Port::is_user_port(8080));
        assert!(!Port::is_user_port(80));

        assert!(Port::is_dynamic(50000));
        assert!(!Port::is_dynamic(8080));
    }

    #[test]
    fn test_well_known_ports() {
        assert_eq!(WellKnown::name(80), Some("http"));
        assert_eq!(WellKnown::name(443), Some("https"));
        assert_eq!(WellKnown::name(22), Some("ssh"));

        assert_eq!(WellKnown::port("http"), Some(80));
        assert_eq!(WellKnown::port("https"), Some(443));
        assert_eq!(WellKnown::port("SSH"), Some(22));
    }

    #[test]
    fn test_port_range() {
        let range = PortRange::new(8000, 8100);
        assert!(range.contains(8050));
        assert!(!range.contains(7999));
        assert!(!range.contains(8101));
        assert_eq!(range.size(), 101);
    }

    #[test]
    fn test_random_port() {
        let port = Port::random_available();
        assert!(port.is_some());
        let port = port.unwrap();
        assert!(port > 0);
    }

    #[test]
    fn test_find_available() {
        let port = Port::find_available(49152);
        assert!(port.is_some());
    }
}
