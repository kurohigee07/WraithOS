//! Orchestrator core library. Binary di main.rs thin wrapper.

pub mod build_api;
pub mod capabilities;
pub mod cli;
pub mod config;
pub mod context;
pub mod error;
pub mod events;
mod fs_util;
pub mod http_api;
pub mod module;
pub mod module_registry;
pub mod quota;
pub mod rhai_worker;
pub mod telemetry;
pub mod tenant;
pub mod ui;
pub mod wasm;
pub mod ws;

pub use capabilities::CapabilitySet;
pub use cli::Cli;
pub use config::{Config, LogFormat, ScriptConfig};
pub use context::{AppContext, AppContextConfig, SocketRegistry, WasmPermit, WasmPool};
pub use error::{OrchestratorError, Result};
pub use events::{EventBus, OrchestratorEvent};
pub use module::OrchestratorModule;
pub use module_registry::ModuleRegistry;
pub use quota::{ModuleQuotaState, TokenBucket};
pub use rhai_worker::RhaiWorker;
pub use tenant::{TenantPermit, TenantRegistry, TenantState};
pub use wasm::module::WasmModule;
