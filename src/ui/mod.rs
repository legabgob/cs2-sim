pub mod builder;
pub mod editor;
pub mod players;
pub mod results;
pub mod setup;

use egui::Context;
use crate::app::App;

/// Main layout: four resizable panels arranged left-to-right.
pub fn show(app: &mut App, ctx: &Context) {
    // Synthetic data notice
    if app.show_synthetic_notice {
        egui::TopBottomPanel::top("synthetic_banner").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(
                    egui::Color32::from_rgb(100, 160, 230),
                    "ℹ  Using synthetic data — HLTV was unreachable. Run: python pipeline/run.py fetch+train  for live data.",
                );
                if ui.small_button("×").clicked() {
                    app.show_synthetic_notice = false;
                }
            });
        });
    }

    // Stale data warning banner at top
    if app.show_stale_warning {
        egui::TopBottomPanel::top("stale_banner").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(
                    egui::Color32::from_rgb(230, 120, 20),
                    "⚠  Data is stale — run: python pipeline/run.py fetch+train  to update.",
                );
                if ui.small_button("×").clicked() {
                    app.show_stale_warning = false;
                }
            });
        });
    }

    // Error toast
    if let Some(ref msg) = app.error_message.clone() {
        egui::TopBottomPanel::top("error_banner").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(egui::Color32::RED, format!("Error: {msg}"));
                if ui.small_button("×").clicked() {
                    app.error_message = None;
                }
            });
        });
    }

    // Top toolbar: Edit Data button
    egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            if ui.button("✏  Edit Data").on_hover_text(
                "Manually edit team stats, player rosters, and match results.\nSaved to data/overrides.json."
            ).clicked() {
                app.editor_open = !app.editor_open;
            }
        });
    });

    // Floating data editor window
    editor::show(app, ctx);

    egui::SidePanel::left("setup_panel")
        .resizable(true)
        .default_width(220.0)
        .min_width(160.0)
        .show(ctx, |ui| {
            ui.heading("Setup");
            ui.separator();
            setup::show(app, ui);
        });

    egui::SidePanel::left("builder_panel")
        .resizable(true)
        .default_width(280.0)
        .min_width(200.0)
        .show(ctx, |ui| {
            ui.heading("Tournament Builder");
            ui.separator();
            builder::show(app, ui);
        });

    egui::SidePanel::left("players_panel")
        .resizable(true)
        .default_width(220.0)
        .min_width(160.0)
        .show(ctx, |ui| {
            ui.heading("Roster");
            ui.separator();
            players::show(app, ui);
        });

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.heading("Results");
        ui.separator();
        results::show(app, ui);
    });
}
