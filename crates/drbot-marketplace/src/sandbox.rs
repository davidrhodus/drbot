//! Sandbox for secure plugin execution.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::{MarketplaceError, Result};

/// Sandbox configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Enable sandboxing.
    pub enabled: bool,
    /// Memory limit in bytes.
    pub memory_limit: usize,
    /// CPU time limit in milliseconds.
    pub cpu_limit_ms: u64,
    /// Enable network access.
    pub allow_network: bool,
    /// Enable file system access.
    pub allow_fs: bool,
    /// Allowed hosts for network.
    pub allowed_hosts: Vec<String>,
    /// Allowed paths for fs.
    pub allowed_paths: Vec<String>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            memory_limit: 64 * 1024 * 1024, // 64MB
            cpu_limit_ms: 5000,             // 5 seconds
            allow_network: false,
            allow_fs: false,
            allowed_hosts: Vec::new(),
            allowed_paths: Vec::new(),
        }
    }
}

/// Sandbox permissions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SandboxPermissions {
    /// Can read files.
    pub read_files: bool,
    /// Can write files.
    pub write_files: bool,
    /// Can access network.
    pub network: bool,
    /// Can access environment variables.
    pub env_vars: bool,
    /// Can spawn processes.
    pub spawn_process: bool,
    /// Can access system info.
    pub system_info: bool,
    /// Specific allowed APIs.
    pub allowed_apis: HashSet<String>,
}

impl SandboxPermissions {
    /// Create minimal permissions.
    pub fn minimal() -> Self {
        Self::default()
    }

    /// Create permissions for reading.
    pub fn read_only() -> Self {
        Self {
            read_files: true,
            ..Default::default()
        }
    }

    /// Create full permissions (not sandboxed).
    pub fn full() -> Self {
        Self {
            read_files: true,
            write_files: true,
            network: true,
            env_vars: true,
            spawn_process: true,
            system_info: true,
            allowed_apis: HashSet::new(),
        }
    }

    /// Check if an API is allowed.
    pub fn is_api_allowed(&self, api: &str) -> bool {
        self.allowed_apis.is_empty() || self.allowed_apis.contains(api)
    }

    /// Add an allowed API.
    pub fn allow_api(&mut self, api: &str) {
        self.allowed_apis.insert(api.to_string());
    }
}

/// Sandbox execution result.
#[derive(Debug, Clone)]
pub struct SandboxResult {
    /// Return value.
    pub value: serde_json::Value,
    /// Memory used in bytes.
    pub memory_used: usize,
    /// CPU time used in milliseconds.
    pub cpu_time_ms: u64,
    /// Warnings.
    pub warnings: Vec<String>,
}

/// Sandbox instance.
pub struct Sandbox {
    config: SandboxConfig,
    permissions: SandboxPermissions,
}

impl Sandbox {
    /// Create a new sandbox.
    pub fn new(config: SandboxConfig) -> Self {
        Self {
            config,
            permissions: SandboxPermissions::minimal(),
        }
    }

    /// Set permissions.
    pub fn with_permissions(mut self, permissions: SandboxPermissions) -> Self {
        self.permissions = permissions;
        self
    }

    /// Check if an operation is allowed.
    pub fn check_permission(&self, permission: &str) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let allowed = match permission {
            "read_files" => self.permissions.read_files,
            "write_files" => self.permissions.write_files,
            "network" => self.permissions.network && self.config.allow_network,
            "env_vars" => self.permissions.env_vars,
            "spawn_process" => self.permissions.spawn_process,
            "system_info" => self.permissions.system_info,
            _ => self.permissions.is_api_allowed(permission),
        };

        if allowed {
            Ok(())
        } else {
            Err(MarketplaceError::PermissionDenied(permission.to_string()))
        }
    }

    /// Check if a host is allowed for network access.
    pub fn check_host(&self, host: &str) -> Result<()> {
        if !self.config.enabled || !self.permissions.network {
            return Err(MarketplaceError::PermissionDenied("network".to_string()));
        }

        if !self.config.allow_network {
            return Err(MarketplaceError::PermissionDenied(
                "network disabled".to_string(),
            ));
        }

        if self.config.allowed_hosts.is_empty() {
            return Ok(());
        }

        for allowed in &self.config.allowed_hosts {
            if host == allowed || host.ends_with(&format!(".{}", allowed)) {
                return Ok(());
            }
        }

        Err(MarketplaceError::PermissionDenied(format!(
            "host not allowed: {}",
            host
        )))
    }

    /// Check if a path is allowed for file access.
    pub fn check_path(&self, path: &str, write: bool) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        if write && !self.permissions.write_files {
            return Err(MarketplaceError::PermissionDenied(
                "write_files".to_string(),
            ));
        }

        if !self.permissions.read_files {
            return Err(MarketplaceError::PermissionDenied("read_files".to_string()));
        }

        if !self.config.allow_fs {
            return Err(MarketplaceError::PermissionDenied(
                "fs disabled".to_string(),
            ));
        }

        if self.config.allowed_paths.is_empty() {
            return Ok(());
        }

        for allowed in &self.config.allowed_paths {
            if path.starts_with(allowed) {
                return Ok(());
            }
        }

        Err(MarketplaceError::PermissionDenied(format!(
            "path not allowed: {}",
            path
        )))
    }

    /// Get memory limit.
    pub fn memory_limit(&self) -> usize {
        self.config.memory_limit
    }

    /// Get CPU limit.
    pub fn cpu_limit_ms(&self) -> u64 {
        self.config.cpu_limit_ms
    }

    /// Check resource limits.
    pub fn check_limits(&self, memory_used: usize, cpu_time_ms: u64) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        if memory_used > self.config.memory_limit {
            return Err(MarketplaceError::SandboxError(format!(
                "memory limit exceeded: {} > {}",
                memory_used, self.config.memory_limit
            )));
        }

        if cpu_time_ms > self.config.cpu_limit_ms {
            return Err(MarketplaceError::SandboxError(format!(
                "CPU time limit exceeded: {} > {}",
                cpu_time_ms, self.config.cpu_limit_ms
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_permissions() {
        let config = SandboxConfig::default();
        let sandbox = Sandbox::new(config);

        // Default permissions deny everything
        assert!(sandbox.check_permission("read_files").is_err());
        assert!(sandbox.check_permission("network").is_err());
    }

    #[test]
    fn test_sandbox_with_permissions() {
        let config = SandboxConfig::default();
        let sandbox = Sandbox::new(config).with_permissions(SandboxPermissions::read_only());

        assert!(sandbox.check_permission("read_files").is_ok());
        assert!(sandbox.check_permission("write_files").is_err());
    }

    #[test]
    fn test_host_checking() {
        let config = SandboxConfig {
            enabled: true,
            allow_network: true,
            allowed_hosts: vec!["api.example.com".to_string()],
            ..Default::default()
        };
        let mut permissions = SandboxPermissions::minimal();
        permissions.network = true;
        let sandbox = Sandbox::new(config).with_permissions(permissions);

        assert!(sandbox.check_host("api.example.com").is_ok());
        assert!(sandbox.check_host("evil.com").is_err());
    }

    #[test]
    fn test_resource_limits() {
        let config = SandboxConfig {
            memory_limit: 1000,
            cpu_limit_ms: 100,
            ..Default::default()
        };
        let sandbox = Sandbox::new(config);

        assert!(sandbox.check_limits(500, 50).is_ok());
        assert!(sandbox.check_limits(2000, 50).is_err());
        assert!(sandbox.check_limits(500, 200).is_err());
    }
}
