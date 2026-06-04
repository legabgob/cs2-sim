use egui::Ui;
use crate::app::App;
use crate::presets;
use crate::types::{RoundConfig, SeriesFormat, StageType, TournamentStage};

pub fn show(app: &mut App, ui: &mut Ui) {
    // ── Preset dropdown ───────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label("Preset:");
        egui::ComboBox::from_id_source("preset_combo")
            .selected_text(app.selected_preset.as_str())
            .show_ui(ui, |ui| {
                for name in presets::preset_names() {
                    if ui.selectable_label(app.selected_preset == *name, *name).clicked() {
                        app.selected_preset = name.to_string();
                        if let Some(fmt) = presets::load_preset(name) {
                            app.format = fmt;
                        }
                    }
                }
            });
    });

    ui.add_space(4.0);

    // ── Save / Load format ────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label("Path:");
        ui.text_edit_singleline(&mut app.format_save_path);
    });
    ui.horizontal(|ui| {
        if ui.button("💾 Save").clicked() { app.save_format(); }
        if ui.button("📂 Load").clicked() { app.load_format(); }
    });

    ui.separator();

    // ── Stage list ────────────────────────────────────────────────────────────
    egui::ScrollArea::vertical().show(ui, |ui| {
        let n_stages = app.format.stages.len();
        let mut to_remove: Option<usize> = None;
        let mut move_up: Option<usize> = None;
        let mut move_down: Option<usize> = None;

        for i in 0..n_stages {
            let header = {
                let s = &app.format.stages[i];
                format!("{}  ({})", s.name, s.stage_type.label())
            };

            egui::CollapsingHeader::new(&header)
                .id_source(("stage_col", i))
                .default_open(true)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if i > 0 && ui.small_button("↑").clicked() { move_up = Some(i); }
                        if i + 1 < n_stages && ui.small_button("↓").clicked() { move_down = Some(i); }
                        if ui.small_button("🗑 Remove").clicked() { to_remove = Some(i); }
                    });
                    stage_editor(ui, &mut app.format.stages[i]);
                });

            ui.add_space(2.0);
        }

        if let Some(idx) = to_remove { app.format.stages.remove(idx); }
        if let Some(idx) = move_up   { app.format.stages.swap(idx, idx - 1); }
        if let Some(idx) = move_down { app.format.stages.swap(idx, idx + 1); }

        ui.add_space(6.0);
        if ui.button("➕ Add Stage").clicked() {
            app.format.stages.push(default_stage(app.format.stages.len()));
        }
    });
}

fn stage_editor(ui: &mut Ui, stage: &mut TournamentStage) {
    ui.horizontal(|ui| {
        ui.label("Name:");
        ui.text_edit_singleline(&mut stage.name);
    });

    ui.horizontal(|ui| {
        ui.label("Type:");
        egui::ComboBox::from_id_source(format!("st_type_{}", stage.name))
            .selected_text(stage.stage_type.label())
            .show_ui(ui, |ui| {
                for &st in StageType::all() {
                    if ui.selectable_label(stage.stage_type == st, st.label()).clicked() {
                        stage.stage_type = st;
                        stage.round_configs = default_round_configs(st);
                    }
                }
            });
    });

    ui.horizontal(|ui| {
        ui.label("Teams in:");
        ui.add(egui::DragValue::new(&mut stage.teams_in).range(2..=64).speed(1.0));
        ui.label("Advancing:");
        ui.add(egui::DragValue::new(&mut stage.teams_advancing).range(1..=32).speed(1.0));
    });
    if stage.teams_advancing > stage.teams_in {
        stage.teams_advancing = stage.teams_in;
    }

    // ── Round configs ─────────────────────────────────────────────────────────
    ui.collapsing("Round formats", |ui| {
        let mut to_remove: Option<usize> = None;
        for (j, rc) in stage.round_configs.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut rc.label);
                egui::ComboBox::from_id_source(format!("rc_{}_{j}", stage.name))
                    .selected_text(rc.format.label())
                    .width(60.0)
                    .show_ui(ui, |ui| {
                        for fmt in [SeriesFormat::Bo1, SeriesFormat::Bo3, SeriesFormat::Bo5] {
                            if ui.selectable_label(rc.format == fmt, fmt.label()).clicked() {
                                rc.format = fmt;
                            }
                        }
                    });
                if ui.small_button("🗑").clicked() { to_remove = Some(j); }
            });
        }
        if let Some(j) = to_remove { stage.round_configs.remove(j); }
        if ui.small_button("+ Round").clicked() {
            stage.round_configs.push(RoundConfig {
                label: format!("Round {}", stage.round_configs.len() + 1),
                format: SeriesFormat::Bo3,
            });
        }
    });
}

fn default_stage(idx: usize) -> TournamentStage {
    TournamentStage {
        name: format!("Stage {}", idx + 1),
        stage_type: StageType::SingleElimination,
        teams_in: 8,
        teams_advancing: 1,
        round_configs: default_round_configs(StageType::SingleElimination),
    }
}

fn default_round_configs(stage_type: StageType) -> Vec<RoundConfig> {
    match stage_type {
        StageType::Swiss => (1..=5)
            .map(|r| RoundConfig {
                label: format!("Round {r}"),
                format: if r == 1 { SeriesFormat::Bo1 } else { SeriesFormat::Bo3 },
            })
            .collect(),
        StageType::GSL => vec![
            RoundConfig { label: "Opening Match".into(),     format: SeriesFormat::Bo1 },
            RoundConfig { label: "Winners Match".into(),     format: SeriesFormat::Bo3 },
            RoundConfig { label: "Elimination Match".into(), format: SeriesFormat::Bo3 },
            RoundConfig { label: "Decider Match".into(),     format: SeriesFormat::Bo3 },
        ],
        StageType::SingleElimination => vec![
            RoundConfig { label: "Quarterfinals".into(), format: SeriesFormat::Bo3 },
            RoundConfig { label: "Semifinals".into(),    format: SeriesFormat::Bo3 },
            RoundConfig { label: "Grand Final".into(),   format: SeriesFormat::Bo5 },
        ],
        StageType::DoubleElimination => vec![
            RoundConfig { label: "Upper/Lower R1".into(), format: SeriesFormat::Bo3 },
            RoundConfig { label: "Upper/Lower R2".into(), format: SeriesFormat::Bo3 },
            RoundConfig { label: "Upper Final".into(),    format: SeriesFormat::Bo3 },
            RoundConfig { label: "Grand Final".into(),    format: SeriesFormat::Bo5 },
        ],
        StageType::RoundRobin => vec![
            RoundConfig { label: "All rounds".into(), format: SeriesFormat::Bo3 },
        ],
    }
}
