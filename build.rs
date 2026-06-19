fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let out_dir = std::env::var("OUT_DIR").unwrap_or_default();

    let is_primary =
        out_dir.starts_with(&manifest_dir) || manifest_dir.contains("/target/package/");
    if !is_primary {
        let chip_features = [
            "esp32", "esp32s2", "esp32s3", "esp32c2", "esp32c3", "esp32c6", "esp32h2",
        ];

        let enabled: Vec<_> = chip_features
            .iter()
            .filter(|f| {
                let key = format!("CARGO_FEATURE_{}", f.to_uppercase().replace('-', "_"));
                std::env::var(key).is_ok()
            })
            .collect();

        if enabled.len() != 1 {
            panic!(
                "\n\nExpected exactly one of the following features to be enabled:\n  {}\n\n",
                chip_features.join(", "),
            );
        }
    }
}
