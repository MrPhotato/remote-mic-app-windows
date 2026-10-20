use std::{env, fs, path::PathBuf};

fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let config_path = root.join("src-tauri/tauri.conf.json");
    let config: serde_json::Value =
        serde_json::from_slice(&fs::read(&config_path).expect("Read product version")).unwrap();
    let version = config["version"].as_str().expect("Product version");
    println!("cargo:rustc-env=SAYALL_SETUP_VERSION={version}");
    println!("cargo:rerun-if-changed={}", config_path.display());

    for name in ["SAYALL_SETUP_PAYLOAD_SHA256", "SAYALL_SETUP_APP_SHA256"] {
        println!("cargo:rerun-if-env-changed={name}");
        let hash = env::var(name).unwrap_or_default();
        if hash.len() != 64 || !hash.bytes().all(|c| c.is_ascii_hexdigit()) {
            if env::var("PROFILE").as_deref() == Ok("release") {
                panic!("{name} must bind the release setup to its verified payload");
            }
            // Debug and unit tests may build without a payload. Runtime installation
            // rejects this value; there is no development bypass for verification.
            println!("cargo:rustc-env={name}=unbound");
        } else {
            println!("cargo:rustc-env={name}={}", hash.to_ascii_lowercase());
        }
    }
    tauri_build::build();
}
