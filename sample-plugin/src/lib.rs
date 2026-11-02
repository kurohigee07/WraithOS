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

        let emit_payload = format!(r#"{{"from":"{}"}}"#, json_escape(&payload));
        let emit_part = match orchestrator::host::events::emit("sample.emitted", &emit_payload) {
            Ok(()) => r#"{"ok":true}"#.to_string(),
            Err(e) => format!(r#"{{"ok":false,"err":"{}"}}"#, json_escape(&e)),
        };

        Ok(format!(
            r#"{{"probe":"{}","http":{},"tcp":{},"emit":{}}}"#,
            json_escape(&payload),
            http_part,
            tcp_part,
            emit_part,
        ))
    }
}

export!(SamplePlugin);
