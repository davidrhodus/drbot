//! Local-only mode for maximum privacy.

use serde::{Deserialize, Serialize};

/// Local mode configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalModeConfig {
    /// Enable local-only mode.
    pub enabled: bool,
    /// Local model to use.
    pub local_model: String,
    /// Ollama endpoint.
    pub ollama_endpoint: String,
    /// Cache responses locally.
    pub cache_responses: bool,
    /// Block all external requests.
    pub block_external: bool,
}

impl Default for LocalModeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            local_model: "llama3".to_string(),
            ollama_endpoint: "http://localhost:11434".to_string(),
            cache_responses: true,
            block_external: true,
        }
    }
}

/// Local mode manager.
pub struct LocalMode {
    config: LocalModeConfig,
}

impl LocalMode {
    /// Create a new local mode manager.
    pub fn new(config: LocalModeConfig) -> Self {
        Self { config }
    }

    /// Check if local mode is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get the local model.
    pub fn local_model(&self) -> &str {
        &self.config.local_model
    }

    /// Get the Ollama endpoint.
    pub fn ollama_endpoint(&self) -> &str {
        &self.config.ollama_endpoint
    }

    /// Check if external requests are blocked.
    pub fn blocks_external(&self) -> bool {
        self.config.enabled && self.config.block_external
    }

    /// Enable local mode.
    pub fn enable(&mut self) {
        self.config.enabled = true;
    }

    /// Disable local mode.
    pub fn disable(&mut self) {
        self.config.enabled = false;
    }

    /// Set local model.
    pub fn set_model(&mut self, model: &str) {
        self.config.local_model = model.to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_mode() {
        let mut mode = LocalMode::new(LocalModeConfig::default());
        assert!(!mode.is_enabled());

        mode.enable();
        assert!(mode.is_enabled());
        assert!(mode.blocks_external());

        mode.disable();
        assert!(!mode.is_enabled());
    }
}
