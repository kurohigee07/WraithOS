use crate::events::OrchestratorEvent;
use crate::wasm::orchestrator::host::{events, http, tcp};
use crate::wasm::state::WasmHostState;

impl http::Host for WasmHostState {
    async fn get(&mut self, url: String) -> Result<String, String> {
        let parsed = reqwest::Url::parse(&url).map_err(|e| format!("invalid url: {e}"))?;
        self.ctx
            .capabilities
            .check_http(&parsed)
            .map_err(|e| e.to_string())?;

        let resp = self
            .ctx
            .http
            .get(parsed)
            .send()
            .await
            .map_err(|e| format!("http send failed: {e}"))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(format!("http status {status}"));
        }
        resp.text().await.map_err(|e| format!("http body read failed: {e}"))
    }
}

impl tcp::Host for WasmHostState {
    async fn connect(&mut self, host: String, port: u16) -> Result<u32, String> {
        self.ctx
            .capabilities
            .check_tcp(&host, port)
            .map_err(|e| e.to_string())?;

        let addr = format!("{host}:{port}");
        let stream = tokio::net::TcpStream::connect(&addr)
            .await
            .map_err(|e| format!("tcp connect {addr} failed: {e}"))?;

        let std_stream = stream
            .into_std()
            .map_err(|e| format!("tcp into_std failed: {e}"))?;

        self.ctx
            .sockets
            .insert(std_stream)
            .map_err(|e| e.to_string())
    }

    async fn close(&mut self, handle: u32) -> Result<(), String> {
        self.ctx
            .sockets
            .remove(handle)
            .ok_or_else(|| format!("socket handle {handle} not found"))?;
        Ok(())
    }
}


impl events::Host for WasmHostState {
    async fn emit(&mut self, kind: String, payload_json: String) -> Result<(), String> {
        self.ctx.capabilities.check_emit_event(&kind).map_err(|e| e.to_string())?;
        let payload: serde_json::Value = serde_json::from_str(&payload_json)
            .map_err(|e| format!("invalid payload json: {e}"))?;
        self.ctx.event_bus.publish(OrchestratorEvent {
            kind,
            payload,
            source_module: self.module_id.clone(),
        });
        Ok(())
    }
}
