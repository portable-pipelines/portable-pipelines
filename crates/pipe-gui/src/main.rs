//! Native window. Windows subsystem so this is not a second console.

#![cfg_attr(windows, windows_subsystem = "windows")]

use std::path::PathBuf;

use pipe_gui::{parse_gui_args, GuiApp, LaunchCfg};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let parsed = match parse_gui_args(&args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let cwd = parsed
        .cwd
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let cfg = LaunchCfg {
        file: parsed.file,
        spec: parsed.spec,
        cwd,
        screenshot: parsed.screenshot,
        screenshot_frames: parsed.screenshot_frames,
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([960.0, 600.0])
            .with_title("Portable Pipelines"),
        renderer: eframe::Renderer::Glow,
        vsync: true,
        centered: true,
        ..Default::default()
    };

    if let Err(e) = eframe::run_native(
        "Portable Pipelines",
        options,
        Box::new(move |cc| Ok(Box::new(GuiApp::new(cc, cfg)))),
    ) {
        eprintln!("pipe-gui: {e}");
        std::process::exit(1);
    }
}
