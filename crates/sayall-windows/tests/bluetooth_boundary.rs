//! Guard the explicit boundary: remote connection recovery must not control
//! the computer's Bluetooth adapter. These checks never access a device.

use std::path::Path;

fn check_rust_sources(directory: &Path) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            check_rust_sources(&path);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            // Historical implementation is retained on disk, outside the
            // module tree. The library entry-point check below prevents linking it.
            if path.file_name().unwrap() == "bluetooth_radio.rs" {
                continue;
            }
            let source = std::fs::read_to_string(&path).unwrap();
            for forbidden in [
                "SetStateAsync(",
                "Radio::RequestAccessAsync(",
                "pnputil",
                "restart-device",
                "cycle_bluetooth_radio",
                "prepare_bluetooth_radio_recovery",
                "refresh_bluetooth_radio_cache",
            ] {
                assert!(
                    !source.contains(forbidden),
                    "{} reintroduces system Bluetooth control: {forbidden}",
                    path.display()
                );
            }
        }
    }
}

#[test]
fn application_and_examples_do_not_control_system_bluetooth() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let entry = std::fs::read_to_string(crate_root.join("src/lib.rs")).unwrap();
    assert!(
        !entry.contains("bluetooth_radio"),
        "the retired adapter-control module must stay outside the library"
    );
    check_rust_sources(&crate_root.join("src"));
    check_rust_sources(&crate_root.join("examples"));
    check_rust_sources(&crate_root.join("../../src-tauri/src"));
}
