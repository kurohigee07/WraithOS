wit_bindgen::generate!({
    path: "../orchestrator/wit",
    world: "plugin",
});

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn extract_alloc_mb(payload: &str) -> Option<usize> {
    let needle = "\"alloc_mb\":";
    let pos = payload.find(needle)?;
    let rest = &payload[pos + needle.len()..];
    let digits: String = rest
        .chars()
        .skip_while(|c| c.is_whitespace())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok()
}

struct SamplePlugin;

impl Guest for SamplePlugin {
    fn run(payload: String) -> Result<String, String> {
        // Fast-path OOM test: alokasi N MB. Jika > wasm_memory_limit_mb,
        // memory.grow trap -> execute return Err.
        if let Some(mb) = extract_alloc_mb(&payload) {
            let bytes = mb.saturating_mul(1024 * 1024);
            let buf: Vec<u8> = vec![0xAA_u8; bytes];
            let checksum: u64 = buf.iter().step_by(4096).map(|&b| b as u64).sum();
            return Ok(format!(
                r#"{{"alloc_mb":{},"checksum":{}}}"#,
                mb, checksum,
            ));
        }

        // Normal path: probe http.get + tcp.connect via host bridge.
        let http_part = match orchestrator::host::http::get("https://example.com") {
            Ok(body) => format!(r#"{{"ok":true,"len":{}}}"#, body.len()),
            Err(e) => format!(r#"{{"ok":false,"err":"{}"}}"#, json_escape(&e)),
        };

        let tcp_part = match orchestrator::host::tcp::connect("example.com", 443) {
            Ok(handle) => {
                let closed = orchestrator::host::tcp::close(handle).is_ok();
                format!(r#"{{"ok":true,"handle":{},"closed":{}}}"#, handle, closed)
            }
            Err(e) => format!(r#"{{"ok":false,"err":"{}"}}"#, json_escape(&e)),
        };

        Ok(format!(
            r#"{{"probe":"{}","http":{},"tcp":{}}}"#,
            json_escape(&payload),
            http_part,
            tcp_part,
        ))
    }
}

export!(SamplePlugin);
