fn main() {
    let manifest = std::path::Path::new("../../target/rc003-helper/manifest.json");
    println!("cargo:rerun-if-changed={}", manifest.display());
    let contents = std::fs::read(manifest).unwrap_or_else(|_| b"{}".to_vec());
    let output =
        std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("rc003-helper-manifest.json");
    std::fs::write(output, contents).expect("write helper payload manifest");
}
