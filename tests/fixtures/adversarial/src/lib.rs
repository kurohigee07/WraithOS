//! Adversarial WASM fixtures for host boundary testing.
//!
//! Each scenario exercises a distinct failure mode:
//!   panic           guest unwraps / panics            host: 500, no crash
//!   bad_json        guest returns non-JSON string     host: 500
//!   memory_bomb     guest allocates >256MB linear     host: wasm trap
//!   cpu_spin        guest infinite loop               host: epoch trap ~60s
//!   cap_violation   http::get to disallowed host      host: 403/denied
//!   tcp_violation   tcp::connect to disallowed port   host: 403/denied
//!
//! Payload: {"scenario":"<name>"}. Unknown → returns normal {"ok":true}.

wit_bindgen::generate!({
    path: "../../../orchestrator/wit",
    world: "plugin",
});

fn extract_scenario(payload: &str) -> &str {
    let key = "\"scenario\":\"";
    if let Some(start) = payload.find(key) {
        let rest = &payload[start + key.len()..];
        if let Some(end) = rest.find('"') {
            return &rest[..end];
        }
    }
    "unknown"
}

struct Adversarial;

impl Guest for Adversarial {
    fn run(payload: String) -> Result<String, String> {
        let scenario = extract_scenario(&payload);
        match scenario {
            "panic" => {
                panic!("adversarial: intentional guest panic");
            }
            "bad_json" => {
                return Ok("this is not valid json at all".to_string());
            }
            "memory_bomb" => {
                // Allocate 16MB chunks until wasmtime memory limit hits.
                let mut held: Vec<Vec<u8>> = Vec::new();
                for _ in 0..64 {
                    held.push(vec![0xAAu8; 16 * 1024 * 1024]);
                }
                return Ok(format!("{{\"allocated\":{}}}", held.len() * 16));
            }
            "cpu_spin" => {
                let mut x: u64 = 0;
                loop {
                    x = x.wrapping_mul(1103515245).wrapping_add(12345);
                    if x == u64::MAX {
                        break;
                    }
                }
                return Ok(format!("{{\"unreachable\":{}}}", x));
            }
            "cap_violation" => {
                let url = "http://evil-attacker.invalid/";
                match orchestrator::host::http::get(url) {
                    Ok(body) => return Ok(format!(
                        "{{\"unexpected\":\"allowed\",\"len\":{}}}", body.len()
                    )),
                    Err(e) => return Err(format!("capability_denied: {e}")),
                }
            }
            "tcp_violation" => {
                match orchestrator::host::tcp::connect("1.2.3.4", 9999) {
                    Ok(handle) => {
                        let _ = orchestrator::host::tcp::close(handle);
                        return Ok("{\"unexpected\":\"allowed\"}".to_string());
                    }
                    Err(e) => return Err(format!("capability_denied: {e}")),
                }
            }
            _ => Ok("{\"ok\":true,\"scenario\":\"unknown\"}".to_string()),
        }
    }
}

export!(Adversarial);
