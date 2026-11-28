//! Adversarial demo plugin for WraithOS.
//!
//! Not malicious in the exploitation sense — it does not attempt to
//! escape the sandbox. It exercises every host capability in a way
//! that the orchestrator should reject, so an operator can watch the
//! denials happen with `wraith-help` / audit logs / metrics.
//!
//! Each attempt returns the denial reason in the plugin's JSON output
//! so the result is self-documenting:
//!
//!     {
//!       "attempts": [
//!         {"cap": "emit_event",   "target": "system.compromised",     "result": "denied: ..."},
//!         {"cap": "http_get",     "target": "https://evil.internal/", "result": "denied: ..."},
//!         {"cap": "tcp_connect",  "target": "evil.internal:22",       "result": "denied: ..."}
//!       ]
//!     }
//!
//! Run it with an empty capability manifest and every attempt should
//! be denied; run it with capabilities granted and the corresponding
//! attempt should succeed (which is itself the operator's problem).

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

struct EvilPlugin;

impl Guest for EvilPlugin {
    fn run(_payload: String) -> Result<String, String> {
        let mut attempts: Vec<String> = Vec::new();

        // 1. Emit an event kind that no reasonable capability manifest
        //    would allow. With an empty capability manifest, this is
        //    denied before the event is queued.
        match orchestrator::host::events::emit(
            "system.compromised",
            r#"{"by":"evil-plugin"}"#,
        ) {
            Ok(()) => attempts.push(
                r#"{"cap":"emit_event","target":"system.compromised","result":"ALLOWED (operator granted this — check your manifest)"}"#
                    .to_string(),
            ),
            Err(e) => attempts.push(format!(
                r#"{{"cap":"emit_event","target":"system.compromised","result":"denied: {}"}}"#,
                json_escape(&e)
            )),
        }

        // 2. HTTP to a host that is not in allowed_hosts.
        match orchestrator::host::http::get("https://evil.internal/") {
            Ok(body) => attempts.push(format!(
                r#"{{"cap":"http_get","target":"https://evil.internal/","result":"ALLOWED ({} bytes)"}}"#,
                body.len()
            )),
            Err(e) => attempts.push(format!(
                r#"{{"cap":"http_get","target":"https://evil.internal/","result":"denied: {}"}}"#,
                json_escape(&e)
            )),
        }

        // 3. TCP to a port that is not in allowed_ports.
        match orchestrator::host::tcp::connect("evil.internal", 22) {
            Ok(handle) => attempts.push(format!(
                r#"{{"cap":"tcp_connect","target":"evil.internal:22","result":"ALLOWED (handle {})"}}"#,
                handle
            )),
            Err(e) => attempts.push(format!(
                r#"{{"cap":"tcp_connect","target":"evil.internal:22","result":"denied: {}"}}"#,
                json_escape(&e)
            )),
        }

        Ok(format!(r#"{{"attempts":[{}]}}"#, attempts.join(",")))
    }
}

export!(EvilPlugin);
