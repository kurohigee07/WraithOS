use crate::events::OrchestratorEvent;
use crate::wasm::orchestrator::host::{events, http, tcp};
use crate::wasm::state::WasmHostState;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
        resp.text()
            .await
            .map_err(|e| format!("http body read failed: {e}"))
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
        let _ = stream.set_nodelay(true);
        self.ctx
            .sockets
            .insert(stream, self.module_id.clone())
            .map_err(|e| e.to_string())
    }

    async fn read(&mut self, handle: u32, max_bytes: u32) -> Result<Vec<u8>, String> {
        let stream_arc = self
            .ctx
            .sockets
            .get_stream(handle, &self.module_id)
            .map_err(|e| e.to_string())?;
        let mut guard = stream_arc.lock().await;
        let mut buf = vec![0u8; max_bytes as usize];
        let n = guard
            .read(&mut buf)
            .await
            .map_err(|e| format!("tcp read failed: {e}"))?;
        drop(guard);
        buf.truncate(n);
        Ok(buf)
    }

    async fn write(&mut self, handle: u32, data: Vec<u8>) -> Result<u32, String> {
        let stream_arc = self
            .ctx
            .sockets
            .get_stream(handle, &self.module_id)
            .map_err(|e| e.to_string())?;
        let mut guard = stream_arc.lock().await;
        guard
            .write_all(&data)
            .await
            .map_err(|e| format!("tcp write failed: {e}"))?;
        let written = data.len() as u32;
        drop(guard);
        Ok(written)
    }

    async fn close(&mut self, handle: u32) -> Result<(), String> {
        self.ctx
            .sockets
            .remove(handle, &self.module_id)
            .ok_or_else(|| format!("socket handle {handle} not found"))?;
        Ok(())
    }
}

impl events::Host for WasmHostState {
    async fn emit(&mut self, kind: String, payload_json: String) -> Result<(), String> {
        self.ctx
            .capabilities
            .check_emit_event(&kind)
            .map_err(|e| e.to_string())?;
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
