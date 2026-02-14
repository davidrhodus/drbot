use crate::tools::{IronToolHost, IronToolHostConfig, IronToolResult};
use anyhow::Context;
use std::path::Path;
use std::time::Duration;
use tracing::{debug, info, warn};
use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxBuilder, WasiView};

mod bindings {
    wasmtime::component::bindgen!({
        path: "wit",
        world: "workflow",
        async: true,
    });
}

#[derive(Debug, Clone)]
pub struct IronRunnerConfig {
    pub timeout: Duration,
    pub fuel: Option<u64>,
    pub max_memory_bytes: Option<usize>,
    pub tools: IronToolHostConfig,
}

impl Default for IronRunnerConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(120),
            fuel: Some(5_000_000_000),
            max_memory_bytes: Some(128 * 1024 * 1024),
            tools: IronToolHostConfig::default(),
        }
    }
}

struct HostState {
    wasi: WasiCtx,
    table: ResourceTable,
    tools: IronToolHost,
    limits: wasmtime::StoreLimits,
}

impl HostState {
    fn new(cfg: IronToolHostConfig, limits: wasmtime::StoreLimits) -> Self {
        let mut wasi_builder = WasiCtxBuilder::new();
        // Inherit stdio so guest code using `std` doesn't immediately fail when
        // touching stdin/stdout/stderr.
        wasi_builder.inherit_stdio();
        let wasi = wasi_builder.build();

        Self {
            wasi,
            table: ResourceTable::new(),
            tools: IronToolHost::new(cfg),
            limits,
        }
    }
}

impl WasiView for HostState {
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.wasi
    }

    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
}

#[async_trait::async_trait]
impl bindings::drbot::iron::host::Host for HostState {
    async fn log(&mut self, level: String, message: String) {
        let lvl = level.trim().to_ascii_lowercase();
        let msg = message.trim_end();
        match lvl.as_str() {
            "error" => warn!(target: "drbot_iron", "[workflow] ERROR: {}", msg),
            "warn" | "warning" => warn!(target: "drbot_iron", "[workflow] {}", msg),
            "debug" => debug!(target: "drbot_iron", "[workflow] {}", msg),
            _ => info!(target: "drbot_iron", "[workflow] {}", msg),
        }
    }

    async fn tool_invoke(&mut self, name: String, args_json: String) -> String {
        let res: IronToolResult = self.tools.tool_invoke(name.as_str(), args_json.as_str()).await;
        res.to_json_string()
    }
}

/// Runs Iron workflow WASM components.
pub struct IronRunner {
    engine: Engine,
}

impl IronRunner {
    pub fn new() -> anyhow::Result<Self> {
        let mut cfg = Config::new();
        cfg.async_support(true);
        cfg.consume_fuel(true);
        cfg.wasm_component_model(true);

        let engine = Engine::new(&cfg).context("failed to initialize wasmtime engine")?;
        Ok(Self { engine })
    }

    pub async fn run_file(
        &self,
        component_path: &Path,
        event_json: &str,
        cfg: IronRunnerConfig,
    ) -> anyhow::Result<String> {
        let component = Component::from_file(&self.engine, component_path)
            .with_context(|| format!("failed to load component: {}", component_path.display()))?;
        self.run_component(&component, event_json, cfg).await
    }

    pub async fn run_bytes(
        &self,
        bytes: &[u8],
        event_json: &str,
        cfg: IronRunnerConfig,
    ) -> anyhow::Result<String> {
        let component = Component::new(&self.engine, bytes).context("failed to load component bytes")?;
        self.run_component(&component, event_json, cfg).await
    }

    async fn run_component(
        &self,
        component: &Component,
        event_json: &str,
        cfg: IronRunnerConfig,
    ) -> anyhow::Result<String> {
        let mut linker = Linker::new(&self.engine);
        wasmtime_wasi::add_to_linker_async(&mut linker).context("failed to add wasi to linker")?;
        bindings::drbot::iron::host::add_to_linker(&mut linker, |s: &mut HostState| s)?;

        let limits = match cfg.max_memory_bytes {
            Some(bytes) => wasmtime::StoreLimitsBuilder::new()
                .memory_size(bytes)
                .trap_on_grow_failure(true)
                .build(),
            None => wasmtime::StoreLimitsBuilder::new().build(),
        };

        let mut store = Store::new(&self.engine, HostState::new(cfg.tools, limits));
        store.limiter(|s: &mut HostState| &mut s.limits);
        if let Some(fuel) = cfg.fuel {
            store.set_fuel(fuel).context("failed to add fuel")?;
        }

        let workflow = bindings::Workflow::instantiate_async(&mut store, component, &linker)
            .await
            .context("failed to instantiate workflow")?;

        let call = workflow.call_run(&mut store, event_json);

        let out = if cfg.timeout.as_millis() > 0 {
            tokio::time::timeout(cfg.timeout, call)
                .await
                .context("workflow timed out")?
                .context("workflow failed")?
        } else {
            call.await.context("workflow failed")?
        };

        Ok(out)
    }
}
