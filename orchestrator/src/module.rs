use crate::context::AppContext;
use async_trait::async_trait;
use shared_types::ModuleManifest;
use std::sync::Arc;

#[async_trait]
pub trait OrchestratorModule: Send + Sync {
    fn manifest(&self) -> &ModuleManifest;
    async fn on_load(&mut self, ctx: Arc<AppContext>) -> anyhow::Result<()>;
    async fn execute(
        &self,
        ctx: Arc<AppContext>,
        payload: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value>;
    async fn on_shutdown(&self) -> anyhow::Result<()>;
}
