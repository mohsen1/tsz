use std::path::Path;

fn main() {
    // Read typescript-versions.json to extract the current npm version string
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let versions_path = Path::new(&manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("scripts/conformance/typescript-versions.json");

    let version = if versions_path.exists() {
        let content = std::fs::read_to_string(&versions_path).unwrap();
        let json: serde_json::Value = serde_json::from_str(&content).unwrap();
        let current_sha = json["current"].as_str().unwrap().to_string();
        json["mappings"][&current_sha]["npm"]
            .as_str()
            .unwrap_or("6.0.0-dev")
            .to_string()
    } else {
        "6.0.0-dev".to_string()
    };

    println!("cargo:rustc-env=TSZ_TSC_VERSION={version}");
    println!("cargo:rerun-if-changed={}", versions_path.display());
}
