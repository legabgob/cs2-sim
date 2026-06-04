use egui::Ui;
use crate::app::App;
use crate::types::MapStatus;

pub fn show(app: &mut App, ui: &mut Ui) {
    // Cache age indicator
    if app.data.cache_age_days < f32::MAX {
        let age = app.data.cache_age_days;
        let color = if age > 14.0 {
            egui::Color32::from_rgb(230, 120, 20)
        } else {
            egui::Color32::from_rgb(80, 200, 80)
        };
        ui.colored_label(color, format!("Cache age: {age:.1} days"));
    } else {
        ui.colored_label(egui::Color32::RED, "No cache — run fetch");
    }

    ui.add_space(6.0);

    // ── Team picker ───────────────────────────────────────────────────────────
    ui.collapsing("Teams  (drag to reorder)", |ui| {
        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
            let _n = app.all_teams.len();
            for team in &app.all_teams {
                let mut selected = app.selected_teams.contains(team);
                if ui.checkbox(&mut selected, team.as_str()).changed() {
                    if selected {
                        app.selected_teams.push(team.clone());
                    } else {
                        app.selected_teams.retain(|t| t != team);
                    }
                }
            }
        });
        ui.label(format!("{} teams selected", app.selected_teams.len()));
    });

    ui.add_space(4.0);

    // ── Seeding order ─────────────────────────────────────────────────────────
    ui.collapsing("Seeding order", |ui| {
        egui::ScrollArea::vertical().max_height(250.0).show(ui, |ui| {
            let mut i = 0;
            while i < app.selected_teams.len() {
                ui.horizontal(|ui| {
                    ui.label(format!("{}.", i + 1));
                    ui.label(app.selected_teams[i].as_str());
                    let can_up = i > 0;
                    let can_down = i + 1 < app.selected_teams.len();
                    if can_up && ui.small_button("↑").clicked() {
                        app.selected_teams.swap(i, i - 1);
                    }
                    if can_down && ui.small_button("↓").clicked() {
                        app.selected_teams.swap(i, i + 1);
                    }
                });
                i += 1;
            }
        });
    });

    ui.add_space(4.0);

    // ── Map pool ──────────────────────────────────────────────────────────────
    ui.collapsing("Map Pool", |ui| {
        let maps: Vec<String> = app.data.active_maps.clone();
        for map in &maps {
            let status = app.map_statuses.entry(map.clone()).or_insert(MapStatus::Active);
            let mut active = *status == MapStatus::Active;
            ui.horizontal(|ui| {
                if ui.checkbox(&mut active, map.as_str()).changed() {
                    if let Some(s) = app.map_statuses.get_mut(map) {
                        *s = if active { MapStatus::Active } else { MapStatus::Permabanned };
                    }
                }
                if !active {
                    ui.colored_label(egui::Color32::from_rgb(180, 60, 60), "permabanned");
                }
            });
        }
    });

    ui.add_space(6.0);

    // ── Recency bias ──────────────────────────────────────────────────────────
    ui.label("Recency bias");
    ui.horizontal(|ui| {
        ui.label("Historical");
        ui.add(egui::Slider::new(&mut app.recency_bias, 0.0..=1.0).show_value(false));
        ui.label("Recent");
    });

    ui.add_space(4.0);

    // ── Monte Carlo runs ──────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label("MC Runs:");
        ui.add(
            egui::DragValue::new(&mut app.mc_runs)
                .range(1_000..=100_000)
                .speed(500.0),
        );
    });

    ui.add_space(8.0);

    // ── Run button ────────────────────────────────────────────────────────────
    let run_label = if app.simulation_running { "Running…" } else { "▶  Run Simulation" };
    let run_enabled = !app.simulation_running && !app.selected_teams.is_empty();
    if ui.add_enabled(run_enabled, egui::Button::new(run_label)).clicked() {
        app.run_simulation(ui.ctx().clone());
    }
}
