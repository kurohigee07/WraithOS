//! CPU-bound guest for benchmark. Parses synthetic payload and does
//! accumulator work. Not a real JSON parser, just CPU load with a
//! deterministic result so the latency number is comparable across runs.

wit_bindgen::generate!({
    path: "/home/opc/orchestrator-project/orchestrator/wit",
    world: "plugin",
});

struct JsonHeavy;

impl Guest for JsonHeavy {
    fn run(payload: String) -> Result<String, String> {
        let n = payload.len().max(1);
        let mut acc: u64 = 0;
        for i in 0..200u64 {
            for byte in payload.bytes() {
                acc = acc.wrapping_add((byte as u64).wrapping_mul(i + 1));
            }
        }
        let out = format!(
            r#"{{"sum":{},"len":{},"iters":200}}"#,
            acc, n
        );
        Ok(out)
    }
}

export!(JsonHeavy);
