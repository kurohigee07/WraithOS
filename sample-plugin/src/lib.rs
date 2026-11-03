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

struct SamplePlugin;

impl Guest for SamplePlugin {
    fn run(payload: String) -> Result<String, String> {
        let mut parts: Vec<String> = Vec::new();
        parts.push(format!(r#""probe":"{}""#, json_escape(&payload)));

        // 1. HTTP via reqwest bridge
        let http_part = match orchestrator::host::http::get("https://example.com") {
            Ok(body) => format!(r#""http":{{"ok":true,"len":{}}}"#, body.len()),
            Err(e) => format!(r#""http":{{"ok":false,"err":"{}"}}"#, json_escape(&e)),
        };
        parts.push(http_part);

        // 2. Raw TCP: real HTTP GET over port 80
        let tcp_part = match orchestrator::host::tcp::connect("example.com", 80) {
            Ok(handle) => {
                let req = b"GET / HTTP/1.0\r\nHost: example.com\r\n\r\n";
                let write_res = orchestrator::host::tcp::write(handle, req);
                let read_res = orchestrator::host::tcp::read(handle, 8192);
                let close_res = orchestrator::host::tcp::close(handle);
                match (write_res, read_res, close_res) {
                    (Ok(written), Ok(response), Ok(())) => {
                        let preview_len = response.len().min(120);
                        let preview = String::from_utf8_lossy(&response[..preview_len]);
                        format!(
                            r#""tcp":{{"ok":true,"written":{},"read_len":{},"preview":"{}"}}"#,
                            written,
                            response.len(),
                            json_escape(&preview),
                        )
                    }
                    (w, r, c) => format!(
                        r#""tcp":{{"ok":false,"write_err":"{}","read_err":"{}","close_err":"{}"}}"#,
                        json_escape(&w.err().unwrap_or_default()),
                        json_escape(&r.err().unwrap_or_default()),
                        json_escape(&c.err().unwrap_or_default()),
                    ),
                }
            }
            Err(e) => format!(r#""tcp":{{"ok":false,"err":"{}"}}"#, json_escape(&e)),
        };
        parts.push(tcp_part);

        // 3. Emit event
        let emit_part = match orchestrator::host::events::emit("sample.emitted", "{}") {
            Ok(()) => r#""emit":{"ok":true}"#.to_string(),
            Err(e) => format!(r#""emit":{{"ok":false,"err":"{}"}}"#, json_escape(&e)),
        };
        parts.push(emit_part);

        Ok(format!("{{{}}}", parts.join(",")))
    }
}

export!(SamplePlugin);
