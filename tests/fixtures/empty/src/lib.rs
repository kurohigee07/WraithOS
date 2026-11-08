//! Empty guest: returns {} for benchmark parity with README v0.27.1.

wit_bindgen::generate!({
    path: "../../../orchestrator/wit",
    world: "plugin",
});

struct Empty;

impl Guest for Empty {
    fn run(_payload: String) -> Result<String, String> {
        Ok("{}".into())
    }
}

export!(Empty);
