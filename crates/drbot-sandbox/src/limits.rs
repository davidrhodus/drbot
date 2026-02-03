//! Resource limits and policies for sandboxed execution.

use serde::{Deserialize, Serialize};

/// Resource limits for code execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum execution time in milliseconds.
    pub timeout_ms: u64,
    /// Maximum memory usage in bytes.
    pub memory_bytes: u64,
    /// Maximum CPU time in milliseconds.
    pub cpu_time_ms: u64,
    /// Maximum number of processes.
    pub max_processes: u32,
    /// Maximum number of open files.
    pub max_open_files: u32,
    /// Maximum output size in bytes.
    pub max_output_bytes: u64,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            timeout_ms: 30000,               // 30 seconds
            memory_bytes: 256 * 1024 * 1024, // 256 MB
            cpu_time_ms: 10000,              // 10 seconds CPU time
            max_processes: 1,
            max_open_files: 64,
            max_output_bytes: 1024 * 1024, // 1 MB
        }
    }
}

impl ResourceLimits {
    /// Create strict limits for untrusted code.
    pub fn strict() -> Self {
        Self {
            timeout_ms: 5000,
            memory_bytes: 64 * 1024 * 1024,
            cpu_time_ms: 2000,
            max_processes: 1,
            max_open_files: 16,
            max_output_bytes: 64 * 1024,
        }
    }

    /// Create relaxed limits for trusted code.
    pub fn relaxed() -> Self {
        Self {
            timeout_ms: 300000,                   // 5 minutes
            memory_bytes: 2 * 1024 * 1024 * 1024, // 2 GB
            cpu_time_ms: 120000,                  // 2 minutes CPU time
            max_processes: 10,
            max_open_files: 256,
            max_output_bytes: 10 * 1024 * 1024, // 10 MB
        }
    }

    /// Set timeout.
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    /// Set memory limit.
    pub fn with_memory(mut self, memory_bytes: u64) -> Self {
        self.memory_bytes = memory_bytes;
        self
    }
}

/// Network access policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPolicy {
    /// Allow any network access.
    pub allow_network: bool,
    /// Allowed hosts (if network is allowed).
    pub allowed_hosts: Vec<String>,
    /// Allowed ports.
    pub allowed_ports: Vec<u16>,
    /// Block localhost access.
    pub block_localhost: bool,
}

impl Default for NetworkPolicy {
    fn default() -> Self {
        Self {
            allow_network: false,
            allowed_hosts: Vec::new(),
            allowed_ports: Vec::new(),
            block_localhost: true,
        }
    }
}

impl NetworkPolicy {
    /// Create a policy that blocks all network access.
    pub fn deny_all() -> Self {
        Self::default()
    }

    /// Create a policy that allows all network access.
    pub fn allow_all() -> Self {
        Self {
            allow_network: true,
            allowed_hosts: vec!["*".to_string()],
            allowed_ports: vec![],
            block_localhost: false,
        }
    }

    /// Allow specific hosts.
    pub fn allow_hosts(hosts: Vec<String>) -> Self {
        Self {
            allow_network: true,
            allowed_hosts: hosts,
            allowed_ports: vec![80, 443],
            block_localhost: true,
        }
    }

    /// Check if a host:port is allowed.
    pub fn is_allowed(&self, host: &str, port: u16) -> bool {
        if !self.allow_network {
            return false;
        }

        if self.block_localhost && (host == "localhost" || host == "127.0.0.1" || host == "::1") {
            return false;
        }

        let host_allowed = self.allowed_hosts.is_empty()
            || self.allowed_hosts.contains(&"*".to_string())
            || self.allowed_hosts.iter().any(|h| h == host);

        let port_allowed = self.allowed_ports.is_empty() || self.allowed_ports.contains(&port);

        host_allowed && port_allowed
    }
}

/// Filesystem access policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemPolicy {
    /// Allow any filesystem access.
    pub allow_filesystem: bool,
    /// Read-only paths.
    pub read_only_paths: Vec<String>,
    /// Read-write paths.
    pub read_write_paths: Vec<String>,
    /// Explicitly denied paths.
    pub denied_paths: Vec<String>,
    /// Temporary directory path.
    pub temp_dir: Option<String>,
}

impl Default for FilesystemPolicy {
    fn default() -> Self {
        Self {
            allow_filesystem: true,
            read_only_paths: vec![],
            read_write_paths: vec![],
            denied_paths: vec![
                "/etc".to_string(),
                "/var".to_string(),
                "/usr".to_string(),
                "/bin".to_string(),
                "/sbin".to_string(),
                "/root".to_string(),
            ],
            temp_dir: None,
        }
    }
}

impl FilesystemPolicy {
    /// Create a policy that denies all filesystem access.
    pub fn deny_all() -> Self {
        Self {
            allow_filesystem: false,
            read_only_paths: vec![],
            read_write_paths: vec![],
            denied_paths: vec![],
            temp_dir: None,
        }
    }

    /// Create a policy that allows only a temp directory.
    pub fn temp_only(temp_dir: String) -> Self {
        Self {
            allow_filesystem: true,
            read_only_paths: vec![],
            read_write_paths: vec![temp_dir.clone()],
            denied_paths: vec![],
            temp_dir: Some(temp_dir),
        }
    }

    /// Check if a path can be read.
    pub fn can_read(&self, path: &str) -> bool {
        if !self.allow_filesystem {
            return false;
        }

        // Check denied paths
        for denied in &self.denied_paths {
            if path.starts_with(denied) {
                return false;
            }
        }

        // Check read-only or read-write paths
        if !self.read_only_paths.is_empty() || !self.read_write_paths.is_empty() {
            return self.read_only_paths.iter().any(|p| path.starts_with(p))
                || self.read_write_paths.iter().any(|p| path.starts_with(p));
        }

        true
    }

    /// Check if a path can be written.
    pub fn can_write(&self, path: &str) -> bool {
        if !self.allow_filesystem {
            return false;
        }

        // Check denied paths
        for denied in &self.denied_paths {
            if path.starts_with(denied) {
                return false;
            }
        }

        // Only read-write paths can be written
        if !self.read_write_paths.is_empty() {
            return self.read_write_paths.iter().any(|p| path.starts_with(p));
        }

        // If no specific paths configured, allow writes (except denied)
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_limits_default() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.timeout_ms, 30000);
        assert_eq!(limits.max_processes, 1);
    }

    #[test]
    fn test_network_policy_deny_all() {
        let policy = NetworkPolicy::deny_all();
        assert!(!policy.is_allowed("example.com", 80));
    }

    #[test]
    fn test_network_policy_allowed_hosts() {
        let policy = NetworkPolicy::allow_hosts(vec!["api.example.com".to_string()]);
        assert!(policy.is_allowed("api.example.com", 443));
        assert!(!policy.is_allowed("other.com", 443));
        assert!(!policy.is_allowed("localhost", 80));
    }

    #[test]
    fn test_filesystem_policy_temp_only() {
        let policy = FilesystemPolicy::temp_only("/tmp/sandbox".to_string());
        assert!(policy.can_read("/tmp/sandbox/file.txt"));
        assert!(policy.can_write("/tmp/sandbox/file.txt"));
        assert!(!policy.can_write("/etc/passwd"));
    }
}
