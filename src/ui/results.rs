use egui::Ui;
use egui_plot::{Bar, BarChart, Legend, Plot};

use crate::app::{App, ResultsTab};

pub fn show(app: &mut App, ui: &mut Ui) {
    if app.simulation_running {
        ui.spinner();
        ui.label("Running simulation…");
        return;
    }

    let Some(_results) = &app.results else {
        ui.colored_label(egui::Color32::from_rgb(130, 130, 130), "No results yet — configure and click Run Simulation.");
        return;
    };

    // ── Tab bar ───────────────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        if ui.selectable_label(app.results_tab == ResultsTab::WinProbs,   "Win Probabilities").clicked() {
            app.results_tab = ResultsTab::WinProbs;
        }
        if ui.selectable_label(app.results_tab == ResultsTab::StageReach, "Stage Reach").clicked() {
            app.results_tab = ResultsTab::StageReach;
        }
        if ui.selectable_label(app.results_tab == ResultsTab::PlayerPerf, "Player Performance").clicked() {
            app.results_tab = ResultsTab::PlayerPerf;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Export CSV").clicked() {
                if let Some(csv) = app.export_results_csv() {
                    let _ = std::fs::write("data/results.csv", &csv);
                }
            }
        });
    });

    ui.separator();

    // Clone needed data to avoid borrow issues
    let results = app.results.as_ref().unwrap();

    match app.results_tab {
        ResultsTab::WinProbs  => show_win_probs(ui, results),
        ResultsTab::StageReach => show_stage_reach(ui, results),
        ResultsTab::PlayerPerf => show_player_perf(ui, results),
    }
}

// ── Win probability bar chart ─────────────────────────────────────────────────

fn show_win_probs(ui: &mut Ui, results: &crate::types::SimulationResults) {
    let mut sorted: Vec<(&String, &f32)> = results.win_prob.iter().collect();
    sorted.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));

    if sorted.is_empty() {
        ui.label("No win probability data.");
        return;
    }

    let bars: Vec<Bar> = sorted.iter().enumerate()
        .map(|(i, (name, &prob))| {
            Bar::new(i as f64, (prob * 100.0) as f64)
                .name(name.as_str())
                .width(0.7)
        })
        .collect();

    let team_labels: Vec<&str> = sorted.iter().map(|(n, _)| n.as_str()).collect();

    Plot::new("win_prob_chart")
        .height(260.0)
        .legend(Legend::default())
        .x_axis_formatter(move |mark, _range| {
            let idx = mark.value.round() as usize;
            team_labels.get(idx).copied().unwrap_or("").to_string()
        })
        .y_axis_label("Win Probability (%)")
        .show(ui, |plot_ui| {
            plot_ui.bar_chart(BarChart::new(bars).color(egui::Color32::from_rgb(70, 130, 200)));
        });

    ui.separator();

    // Text table below chart
    egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
        egui::Grid::new("win_prob_table")
            .num_columns(2)
            .striped(true)
            .spacing([12.0, 4.0])
            .show(ui, |ui| {
                ui.strong("Team");
                ui.strong("Win %");
                ui.end_row();
                for (team, &prob) in &sorted {
                    ui.label(team.as_str());
                    ui.label(format!("{:.2}%", prob * 100.0));
                    ui.end_row();
                }
            });
    });
}

// ── Stage reach table ─────────────────────────────────────────────────────────

fn show_stage_reach(ui: &mut Ui, results: &crate::types::SimulationResults) {
    if results.stage_reach.is_empty() {
        ui.label("No stage reach data.");
        return;
    }

    // Collect stages in a stable order (they may not be in insertion order)
    let mut stages: Vec<&String> = results.stage_reach.keys().collect();
    stages.sort();

    // Collect all teams from win_prob (they're the selected teams)
    let mut teams: Vec<(&String, &f32)> = results.win_prob.iter().collect();
    teams.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));

    egui::ScrollArea::both().show(ui, |ui| {
        egui::Grid::new("stage_reach_grid")
            .num_columns(stages.len() + 1)
            .striped(true)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                ui.strong("Team");
                for stage in &stages {
                    ui.strong(stage.as_str());
                }
                ui.end_row();

                for (team, _) in &teams {
                    ui.label(team.as_str());
                    for stage in &stages {
                        let prob = results.stage_reach
                            .get(*stage)
                            .and_then(|m| m.get(*team))
                            .copied()
                            .unwrap_or(0.0);
                        let color = probability_color(prob);
                        ui.colored_label(color, format!("{:.1}%", prob * 100.0));
                    }
                    ui.end_row();
                }
            });
    });
}

// ── Player performance table ──────────────────────────────────────────────────

fn show_player_perf(ui: &mut Ui, results: &crate::types::SimulationResults) {
    if results.player_perf.is_empty() {
        ui.label("No player performance data.");
        return;
    }

    let mut players: Vec<(&String, (f32, f32))> = results.player_perf.iter()
        .map(|(n, &(m, s))| (n, (m, s)))
        .collect();
    players.sort_by(|a, b| b.1.0.partial_cmp(&a.1.0).unwrap_or(std::cmp::Ordering::Equal));

    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("player_perf_grid")
            .num_columns(3)
            .striped(true)
            .spacing([12.0, 4.0])
            .show(ui, |ui| {
                ui.strong("Player");
                ui.strong("Mean Rating");
                ui.strong("± Std");
                ui.end_row();

                for (name, (mean, std)) in &players {
                    ui.label(name.as_str());
                    let color = rating_color(*mean);
                    ui.colored_label(color, format!("{mean:.3}"));
                    ui.label(format!("{std:.3}"));
                    ui.end_row();
                }
            });
    });
}

// ── Color helpers ─────────────────────────────────────────────────────────────

fn probability_color(p: f32) -> egui::Color32 {
    let r = ((1.0 - p) * 220.0) as u8;
    let g = (p * 200.0) as u8;
    egui::Color32::from_rgb(r, g, 50)
}

fn rating_color(r: f32) -> egui::Color32 {
    if r >= 1.20 { egui::Color32::from_rgb(255, 215, 0) }
    else if r >= 1.10 { egui::Color32::from_rgb(80, 200, 80) }
    else if r >= 1.00 { egui::Color32::from_rgb(180, 220, 180) }
    else { egui::Color32::from_rgb(200, 130, 130) }
}
