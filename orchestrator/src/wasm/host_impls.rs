use crate::events::OrchestratorEvent;
use crate::wasm::orchestrator::host::{events, http, tcp};
use crate::wasm::state::WasmHostState;
use futures::StreamExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Hard cap on the size of an HTTP response body read by the
/// `http::get` capability. A hostile (or misbehaving) host in
/// `allowed_hosts` cannot allocate more than this in the orchestrator
/// process. 16 MiB is comfortably above any realistic API response we
/// have seen and well under the default container memory limit.
/// See docs/security.md -> "HTTP body limit (M2)".
const MAX_HTTP_BODY_BYTES: usize = 16 * 1024 * 1024;

/// Helper extracted so the limit is unit-testable without spinning up
/// an HTTP server. Called once per chunk in the streaming read.
fn check_body_size(total: usize) -> Result<(), String> {
    if total > MAX_HTTP_BODY_BYTES {
        Err(format!(
            "http body too large (> {MAX_HTTP_BODY_BYTES} bytes)"
        ))
    } else {
        Ok(())
    }
}

impl http::Host for WasmHostState {
    async fn get(&mut self, url: String) -> Result<String, String> {
        let parsed = reqwest::Url::parse(&url).map_err(|e| format!("invalid url: {e}"))?;
        self.capabilities
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
        // Fast path: if the server advertised Content-Length, reject
        // oversized bodies before reading a byte. This is an
        // optimization, not the enforcement: Content-Length is
        // optional (chunked encoding) and with gzip/brotli auto-
        // decompression it reflects the wire size, not the
        // decompressed size. The streaming cap below is what actually
        // bounds the allocation.
        if let Some(len) = resp.content_length() {
            if len > MAX_HTTP_BODY_BYTES as u64 {
                return Err(format!(
                    "http body too large (content-length {len} > {MAX_HTTP_BODY_BYTES})"
                ));
            }
        }
        // Streaming read with cumulative cap. Applies to both plain
        // and decompressed bodies (reqwest's gzip/brotli features
        // yield decompressed chunks from `bytes_stream`), so a server
        // that compresses 1 GiB into a few MiB on the wire cannot
        // exhaust the process.
        let mut stream = resp.bytes_stream();
        let mut total = 0usize;
        let mut body: Vec<u8> = Vec::new();
        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.map_err(|e| format!("http body chunk: {e}"))?;
            total = total.saturating_add(chunk.len());
            check_body_size(total)?;
            body.extend_from_slice(&chunk);
        }
        String::from_utf8(body).map_err(|e| format!("http body not utf8: {e}"))
    }
    async fn post(
        &mut self,
        url: String,
        body: Vec<u8>,
        content_type: String,
    ) -> Result<String, String> {
        let parsed = reqwest::Url::parse(&url).map_err(|e| format!("invalid url: {e}"))?;
        self.capabilities
            .check_http_post(&parsed)
            .map_err(|e| e.to_string())?;
        let resp = self
            .ctx
            .http
            .post(parsed)
            .header("Content-Type", content_type)
            .body(body)
            .send()
            .await
            .map_err(|e| format!("http send failed: {e}"))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(format!("http status {status}"));
        }
        if let Some(len) = resp.content_length() {
            if len > MAX_HTTP_BODY_BYTES as u64 {
                return Err(format!(
                    "http body too large (content-length {len} > {MAX_HTTP_BODY_BYTES})"
                ));
            }
        }
        let mut stream = resp.bytes_stream();
        let mut total = 0usize;
        let mut out: Vec<u8> = Vec::new();
        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.map_err(|e| format!("http body chunk: {e}"))?;
            total = total.saturating_add(chunk.len());
            check_body_size(total)?;
            out.extend_from_slice(&chunk);
        }
        String::from_utf8(out).map_err(|e| format!("http body not utf8: {e}"))
    }
}

impl tcp::Host for WasmHostState {
    async fn connect(&mut self, host: String, port: u16) -> Result<u32, String> {
        self.capabilities
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
        self.capabilities
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

#[cfg(test)]
mod body_limit_tests {
    use super::*;

    #[test]
    fn accepts_at_limit() {
        assert!(check_body_size(MAX_HTTP_BODY_BYTES).is_ok());
    }

    #[test]
    fn rejects_over_limit() {
        assert!(check_body_size(MAX_HTTP_BODY_BYTES + 1).is_err());
    }

    #[test]
    fn rejects_grossly_oversized() {
        assert!(check_body_size(usize::MAX).is_err());
    }

    #[test]
    fn accepts_small() {
        assert!(check_body_size(1024).is_ok());
    }
}
