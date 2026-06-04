mod app;
mod data;
mod inference;
mod overrides;
mod presets;
mod simulation;
mod types;
mod ui;
mod veto;

use std::path::PathBuf;
use eframe::{egui, NativeOptions};

struct Cs2Sim {
    inner: app::App,
}

impl Cs2Sim {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_zoom_factor(1.0);
        let data_dir = find_data_dir();
        Cs2Sim { inner: app::App::new(data_dir) }
    }
}

impl eframe::App for Cs2Sim {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Poll background simulation thread
        if self.inner.poll_simulation() {
            ctx.request_repaint();
        }
        // Keep repainting while simulation is running so the spinner animates
        if self.inner.simulation_running {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        ui::show(&mut self.inner, ctx);
    }
}

fn find_data_dir() -> PathBuf {
    let cwd_data = PathBuf::from("data");
    if cwd_data.exists() { return cwd_data; }
    if let Ok(exe) = std::env::current_exe() {
        for ancestor in exe.ancestors().skip(1).take(4) {
            let p = ancestor.join("data");
            if p.exists() { return p; }
        }
    }
    PathBuf::from("data")
}

fn main() -> eframe::Result<()> {
    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("CS2 Tournament Simulator")
            .with_inner_size([1400.0, 800.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "CS2 Tournament Simulator",
        options,
        Box::new(|cc| Ok(Box::new(Cs2Sim::new(cc)))),
    )
}
