//! Gateway server implementation.

use crate::router::create_router;
use crate::state::GatewayState;
use drbot_core::Config;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::ServerConfig;
use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;
use tokio::net::TcpListener;
use tracing::info;

async fn load_tls_config_tls13(
    cert_path: &Path,
    key_path: &Path,
) -> Result<axum_server::tls_rustls::RustlsConfig, String> {
    let cert_bytes = tokio::fs::read(cert_path)
        .await
        .map_err(|e| format!("failed to read TLS cert: {}", e))?;
    let key_bytes = tokio::fs::read(key_path)
        .await
        .map_err(|e| format!("failed to read TLS key: {}", e))?;

    let cert_chain: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(&cert_bytes)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("failed to parse TLS cert: {}", e))?;

    let mut key_result: Result<PrivateKeyDer<'static>, String> =
        Err("private key file contained no keys".to_string());
    for item in <PrivateKeyDer<'static> as PemObject>::pem_slice_iter(&key_bytes) {
        let key: Result<PrivateKeyDer<'static>, String> =
            item.map_err(|e| format!("failed to parse TLS key PEM: {}", e));
        match key_result {
            Ok(_) => {
                if key.is_ok() {
                    return Err(
                        "TLS key file contains multiple keys (must contain exactly one)"
                            .to_string(),
                    );
                }
            }
            Err(_) => key_result = key,
        }
    }
    let key_der = key_result?;

    let mut server_config =
        ServerConfig::builder_with_protocol_versions(&[&rustls::version::TLS13])
            .with_no_client_auth()
            .with_single_cert(cert_chain, key_der)
            .map_err(|e| format!("invalid TLS cert/key: {}", e))?;
    server_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    Ok(axum_server::tls_rustls::RustlsConfig::from_config(
        std::sync::Arc::new(server_config),
    ))
}

/// The main gateway server.
pub struct Gateway {
    config: Config,
    state: GatewayState,
}

impl Gateway {
    /// Create a new gateway with the given configuration.
    pub fn new(config: Config) -> Self {
        let state = GatewayState::new(config.clone());
        Self { config, state }
    }

    /// Create a gateway builder.
    pub fn builder() -> GatewayBuilder {
        GatewayBuilder::new()
    }

    /// Get a handle to the shared gateway state.
    pub fn state(&self) -> GatewayState {
        self.state.clone()
    }

    /// Get the server address.
    pub fn addr(&self) -> SocketAddr {
        format!("{}:{}", self.config.gateway.host, self.config.gateway.port)
            .parse()
            .expect("Invalid address")
    }

    /// Run the gateway server.
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        enforce_gateway_bind_policy(&self.config)?;
        let addr = self.addr();
        // Best-effort background services for OpenClaw interop.
        crate::openclaw_inbound::start_inbound_bridge(self.state.clone()).await;

        let router = create_router(self.state);

        let ws_scheme = if self.config.gateway.tls_enabled {
            "wss"
        } else {
            "ws"
        };

        info!("Starting drbot gateway on {}", addr);
        info!("WebSocket endpoint: {}://{}/ws", ws_scheme, addr);
        info!("OpenClaw v3 endpoint: {}://{}/openclaw/ws", ws_scheme, addr);

        if self.config.gateway.tls_enabled {
            let cert =
                self.config.gateway.tls_cert.as_ref().ok_or_else(|| {
                    config_error("gateway.tls_enabled is true but tls_cert is unset")
                })?;
            let key =
                self.config.gateway.tls_key.as_ref().ok_or_else(|| {
                    config_error("gateway.tls_enabled is true but tls_key is unset")
                })?;
            let tls_config = load_tls_config_tls13(cert.as_path(), key.as_path())
                .await
                .map_err(|e| config_error(&e))?;
            axum_server::bind_rustls(addr, tls_config)
                .serve(router.into_make_service_with_connect_info::<SocketAddr>())
                .await?;
        } else {
            let listener = TcpListener::bind(addr).await?;
            axum::serve(
                listener,
                router.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await?;
        }

        Ok(())
    }

    /// Run the gateway server with graceful shutdown.
    pub async fn run_with_shutdown(
        self,
        shutdown: impl std::future::Future<Output = ()> + Send + 'static,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        enforce_gateway_bind_policy(&self.config)?;
        let addr = self.addr();
        // Best-effort background services for OpenClaw interop.
        crate::openclaw_inbound::start_inbound_bridge(self.state.clone()).await;

        let router = create_router(self.state);

        let ws_scheme = if self.config.gateway.tls_enabled {
            "wss"
        } else {
            "ws"
        };

        info!("Starting drbot gateway on {}", addr);
        info!("WebSocket endpoint: {}://{}/ws", ws_scheme, addr);
        info!("OpenClaw v3 endpoint: {}://{}/openclaw/ws", ws_scheme, addr);

        if self.config.gateway.tls_enabled {
            let cert =
                self.config.gateway.tls_cert.as_ref().ok_or_else(|| {
                    config_error("gateway.tls_enabled is true but tls_cert is unset")
                })?;
            let key =
                self.config.gateway.tls_key.as_ref().ok_or_else(|| {
                    config_error("gateway.tls_enabled is true but tls_key is unset")
                })?;
            let tls_config = load_tls_config_tls13(cert.as_path(), key.as_path())
                .await
                .map_err(|e| config_error(&e))?;

            let handle = axum_server::Handle::new();
            let shutdown_handle = handle.clone();
            tokio::spawn(async move {
                shutdown.await;
                shutdown_handle.graceful_shutdown(Some(Duration::from_secs(5)));
            });

            axum_server::bind_rustls(addr, tls_config)
                .handle(handle)
                .serve(router.into_make_service_with_connect_info::<SocketAddr>())
                .await?;
        } else {
            let listener = TcpListener::bind(addr).await?;
            axum::serve(
                listener,
                router.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(shutdown)
            .await?;
        }

        info!("Gateway shutdown complete");
        Ok(())
    }
}

fn config_error(message: &str) -> Box<dyn std::error::Error + Send + Sync> {
    Box::new(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        message.to_string(),
    ))
}

fn is_loopback_host(host: &str) -> bool {
    let trimmed = host.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let candidate = trimmed
        .strip_prefix('[')
        .and_then(|v| v.strip_suffix(']'))
        .unwrap_or(trimmed);
    match candidate.parse::<std::net::IpAddr>() {
        Ok(ip) => ip.is_loopback(),
        Err(_) => false,
    }
}

fn is_auth_token_configured(cfg: &Config) -> bool {
    cfg.gateway
        .auth_token
        .as_deref()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .is_some()
}

/// Safety policy aligned with OpenClaw's gateway behavior:
/// refuse to bind publicly without auth.
fn enforce_gateway_bind_policy(
    cfg: &Config,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let host = cfg.gateway.host.trim();
    if !is_loopback_host(host) && !is_auth_token_configured(cfg) {
        return Err(config_error(&format!(
            "Refusing to bind gateway to {}:{} without auth. Set gateway.auth_token (or bind to 127.0.0.1).",
            cfg.gateway.host, cfg.gateway.port
        )));
    }

    if cfg.gateway.tls_enabled {
        if cfg.gateway.tls_cert.is_none() || cfg.gateway.tls_key.is_none() {
            return Err(config_error(
                "gateway.tls_enabled is true but tls_cert/tls_key are not both set",
            ));
        }
    }

    Ok(())
}

/// Builder for configuring a Gateway.
pub struct GatewayBuilder {
    config: Option<Config>,
}

impl GatewayBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self { config: None }
    }

    /// Set the configuration.
    pub fn config(mut self, config: Config) -> Self {
        self.config = Some(config);
        self
    }

    /// Build the gateway.
    pub fn build(self) -> Gateway {
        let config = self.config.unwrap_or_default();
        Gateway::new(config)
    }
}

impl Default for GatewayBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gateway_builder() {
        let gateway = Gateway::builder().build();
        assert_eq!(gateway.config.gateway.port, 18789);
    }

    #[test]
    fn test_gateway_addr() {
        let gateway = Gateway::new(Config::default());
        let addr = gateway.addr();
        assert_eq!(addr.port(), 18789);
    }
}
