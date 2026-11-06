wit_bindgen::generate!({ path: "wit", world: "plugin" });

struct Empty;

impl Guest for Empty {
    fn run(_payload: String) -> Result<String, String> {
        Ok("{}".to_string())
    }
}

export!(Empty);
