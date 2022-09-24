#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use anyhow::{Ok, Result};
use rpc::Rpc;

fn main() -> Result<(), anyhow::Error> {
    let options = eframe::NativeOptions {
        min_window_size: Some([1000.0, 700.0].into()),
        drag_and_drop_support: true,
        ..Default::default()
    };
    eframe::run_native("RPC", options, Box::new(|_cc| Box::new(Rpc::default())));
    Ok(())
}
