//! Read-only Bluetooth radio status probe.
//!
//! Usage: cargo run -p sayall-windows --example radio_probe
//! No arguments, access requests, radio changes, or device restarts are supported.

use std::future::IntoFuture;

use windows::Devices::Radios::{Radio, RadioKind};
use windows::Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED};

fn read_bluetooth_radio_state() -> windows::core::Result<()> {
    let radios = futures::executor::block_on(Radio::GetRadiosAsync()?.into_future())?;
    let mut count = 0;
    for index in 0..radios.Size()? {
        let radio = radios.GetAt(index)?;
        if radio.Kind()? == RadioKind::Bluetooth {
            count += 1;
            println!(
                "bluetooth_radio index={count} state={:?} system_radio=unchanged",
                radio.State()?
            );
        }
    }
    println!("bluetooth_radio count={count} operation=read_only");
    Ok(())
}

fn main() -> windows::core::Result<()> {
    if std::env::args_os().nth(1).is_some() {
        eprintln!("This read-only probe accepts no arguments.");
        std::process::exit(2);
    }
    unsafe { RoInitialize(RO_INIT_MULTITHREADED)? };
    let result = read_bluetooth_radio_state();
    unsafe { RoUninitialize() };
    result
}
