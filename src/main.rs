#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use rpc::Rpc;

fn main() -> anyhow::Result<()> {
    let options = eframe::NativeOptions {
        min_window_size: Some([800.0, 600.0].into()),
        drag_and_drop_support: true,
        ..Default::default()
    };
    eframe::run_native("RPC", options, Box::new(|_cc| Box::new(Rpc::default())));
}
