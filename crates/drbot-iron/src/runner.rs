use crate::tools::{IronToolHost, IronToolHostConfig, IronToolResult};
use anyhow::Context;
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;
use tracing::{debug, info, warn};
use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxBuilder, WasiView};

const DEFAULT_EPOCH_TICK: Duration = Duration::from_millis(10);

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
        let res: IronToolResult = self
            .tools
            .tool_invoke(name.as_str(), args_json.as_str())
            .await;
        res.to_json_string()
    }
}

struct EpochTicker {
    stop: Arc<AtomicBool>,
    join: Option<thread::JoinHandle<()>>,
}

impl EpochTicker {
    fn start(engine: Arc<Engine>, tick: Duration) -> anyhow::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();

        let join = thread::Builder::new()
            .name("drbot-iron-epoch".to_string())
            .spawn(move || {
                while !stop_thread.load(Ordering::Relaxed) {
                    thread::sleep(tick);
                    engine.increment_epoch();
                }
            })
            .context("failed to spawn Iron epoch tick thread")?;

        Ok(Self {
            stop,
            join: Some(join),
        })
    }
}

impl Drop for EpochTicker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// Runs Iron workflow WASM components.
pub struct IronRunner {
    engine: Arc<Engine>,
    epoch_tick: Duration,
    _epoch: EpochTicker,
}

/// A workflow component loaded/compiled against a specific [`IronRunner`] engine.
pub struct IronLoadedWorkflow {
    component: Component,
}

#[derive(Debug, Clone)]
pub struct IronRunOutput {
    pub output: String,
    pub fuel_consumed: Option<u64>,
}

impl IronRunner {
    pub fn new() -> anyhow::Result<Self> {
        let mut cfg = Config::new();
        cfg.async_support(true);
        cfg.consume_fuel(true);
        cfg.wasm_component_model(true);
        cfg.epoch_interruption(true);

        let engine = Arc::new(Engine::new(&cfg).context("failed to initialize wasmtime engine")?);
        let epoch_tick = DEFAULT_EPOCH_TICK;
        let epoch = EpochTicker::start(engine.clone(), epoch_tick)?;
        Ok(Self {
            engine,
            epoch_tick,
            _epoch: epoch,
        })
    }

    pub fn load_file(&self, component_path: &Path) -> anyhow::Result<IronLoadedWorkflow> {
        let component = Component::from_file(&self.engine, component_path)
            .with_context(|| format!("failed to load component: {}", component_path.display()))?;
        Ok(IronLoadedWorkflow { component })
    }

    pub fn load_bytes(&self, bytes: &[u8]) -> anyhow::Result<IronLoadedWorkflow> {
        let component =
            Component::new(&self.engine, bytes).context("failed to load component bytes")?;
        Ok(IronLoadedWorkflow { component })
    }

    pub async fn run_file_with_stats(
        &self,
        component_path: &Path,
        event_json: &str,
        cfg: IronRunnerConfig,
    ) -> anyhow::Result<IronRunOutput> {
        let loaded = self.load_file(component_path)?;
        self.run_loaded_with_stats(&loaded, event_json, cfg).await
    }

    pub async fn run_file(
        &self,
        component_path: &Path,
        event_json: &str,
        cfg: IronRunnerConfig,
    ) -> anyhow::Result<String> {
        Ok(self
            .run_file_with_stats(component_path, event_json, cfg)
            .await?
            .output)
    }

    pub async fn run_bytes_with_stats(
        &self,
        bytes: &[u8],
        event_json: &str,
        cfg: IronRunnerConfig,
    ) -> anyhow::Result<IronRunOutput> {
        let loaded = self.load_bytes(bytes)?;
        self.run_loaded_with_stats(&loaded, event_json, cfg).await
    }

    pub async fn run_bytes(
        &self,
        bytes: &[u8],
        event_json: &str,
        cfg: IronRunnerConfig,
    ) -> anyhow::Result<String> {
        Ok(self
            .run_bytes_with_stats(bytes, event_json, cfg)
            .await?
            .output)
    }

    pub async fn run_loaded_with_stats(
        &self,
        loaded: &IronLoadedWorkflow,
        event_json: &str,
        cfg: IronRunnerConfig,
    ) -> anyhow::Result<IronRunOutput> {
        self.run_component_with_stats(&loaded.component, event_json, cfg)
            .await
    }

    pub async fn run_loaded(
        &self,
        loaded: &IronLoadedWorkflow,
        event_json: &str,
        cfg: IronRunnerConfig,
    ) -> anyhow::Result<String> {
        Ok(self
            .run_loaded_with_stats(loaded, event_json, cfg)
            .await?
            .output)
    }

    fn timeout_to_epoch_ticks(&self, timeout: Duration) -> u64 {
        let tick_ms = self.epoch_tick.as_millis().max(1);
        let timeout_ms = timeout.as_millis();
        if timeout_ms == 0 {
            return u64::MAX / 2;
        }
        let ticks = (timeout_ms.saturating_add(tick_ms - 1) / tick_ms) as u64;
        ticks.max(1)
    }

    async fn run_component_with_stats(
        &self,
        component: &Component,
        event_json: &str,
        cfg: IronRunnerConfig,
    ) -> anyhow::Result<IronRunOutput> {
        let timeout = cfg.timeout;
        let fuel = cfg.fuel;
        let max_memory_bytes = cfg.max_memory_bytes;
        let tools_cfg = cfg.tools;

        let mut linker = Linker::new(&self.engine);
        wasmtime_wasi::add_to_linker_async(&mut linker).context("failed to add wasi to linker")?;
        bindings::drbot::iron::host::add_to_linker(&mut linker, |s: &mut HostState| s)?;

        let limits = match max_memory_bytes {
            Some(bytes) => wasmtime::StoreLimitsBuilder::new()
                .memory_size(bytes)
                .trap_on_grow_failure(true)
                .build(),
            None => wasmtime::StoreLimitsBuilder::new().build(),
        };

        let mut store = Store::new(&self.engine, HostState::new(tools_cfg, limits));
        store.limiter(|s: &mut HostState| &mut s.limits);
        store.epoch_deadline_trap();
        store.set_epoch_deadline(self.timeout_to_epoch_ticks(timeout));

        if let Some(fuel) = fuel {
            store.set_fuel(fuel).context("failed to add fuel")?;
        }

        let workflow = bindings::Workflow::instantiate_async(&mut store, component, &linker)
            .await
            .context("failed to instantiate workflow")?;

        match workflow.call_run(&mut store, event_json).await {
            Ok(out) => {
                let fuel_consumed = match fuel {
                    Some(initial) => store
                        .get_fuel()
                        .ok()
                        .map(|remaining| initial.saturating_sub(remaining)),
                    None => None,
                };
                Ok(IronRunOutput {
                    output: out,
                    fuel_consumed,
                })
            }
            Err(e) => {
                for cause in e.chain() {
                    if let Some(trap) = cause.downcast_ref::<wasmtime::Trap>() {
                        if matches!(trap, wasmtime::Trap::Interrupt) {
                            return Err(anyhow::anyhow!(
                                "workflow timed out (epoch deadline reached)"
                            ));
                        }
                        if matches!(trap, wasmtime::Trap::OutOfFuel) {
                            return Err(anyhow::anyhow!("workflow exhausted fuel"));
                        }
                    }
                }
                Err(e).context("workflow failed")
            }
        }
    }
}
