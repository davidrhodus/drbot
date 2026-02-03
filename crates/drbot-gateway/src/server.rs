//! Gateway server implementation.

use crate::router::create_router;
use crate::state::GatewayState;
use drbot_core::Config;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::info;

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

    /// Get the server address.
    pub fn addr(&self) -> SocketAddr {
        format!("{}:{}", self.config.gateway.host, self.config.gateway.port)
            .parse()
            .expect("Invalid address")
    }

    /// Run the gateway server.
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let addr = self.addr();
        let router = create_router(self.state);

        info!("Starting drbot gateway on {}", addr);
        info!("WebSocket endpoint: ws://{}/ws", addr);

        let listener = TcpListener::bind(addr).await?;
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await?;

        Ok(())
    }

    /// Run the gateway server with graceful shutdown.
    pub async fn run_with_shutdown(
        self,
        shutdown: impl std::future::Future<Output = ()> + Send + 'static,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let addr = self.addr();
        let router = create_router(self.state);

        info!("Starting drbot gateway on {}", addr);
        info!("WebSocket endpoint: ws://{}/ws", addr);

        let listener = TcpListener::bind(addr).await?;
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(shutdown)
        .await?;

        info!("Gateway shutdown complete");
        Ok(())
    }
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
