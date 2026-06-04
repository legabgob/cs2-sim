/// Data Editor — floating window to manually add/edit team stats, player
/// rosters, and match results without touching JSON files directly.
///
/// Changes are kept in memory until "Save" is clicked, which writes
/// data/overrides.json.  The user then re-runs `python pipeline/run.py train`
/// to bake the changes into the ONNX models.

use egui::{Color32, Context, DragValue, Grid, ScrollArea, Ui};

use crate::app::App;
use crate::overrides::{MatchEntry, PlayerEntry};
use crate::types::ACTIVE_MAPS;

// ── Tab enum ──────────────────────────────────────────────────────────────────

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum EditorTab { #[default] Teams, Players, Results }

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn show(app: &mut App, ctx: &Context) {
    if !app.editor_open { return; }

    // Copy the flag so we can borrow `app` inside the closure.
    let mut open = true;
    egui::Window::new("✏  Data Editor")
        .open(&mut open)
        .default_size([740.0, 520.0])
        .min_size([500.0, 360.0])
        .resizable(true)
        .show(ctx, |ui| {
            // ── Tab bar ───────────────────────────────────────────────────────
            ui.horizontal(|ui| {
                for (tab, label) in [
                    (EditorTab::Teams,   "🏆 Teams"),
                    (EditorTab::Players, "👤 Players"),
                    (EditorTab::Results, "🎮 Results"),
                ] {
                    if ui.selectable_label(app.editor_tab == tab, label).clicked() {
                        app.editor_tab = tab;
                    }
                }
            });
            ui.separator();

            // ── Tab content ───────────────────────────────────────────────────
            match app.editor_tab {
                EditorTab::Teams   => show_teams(app, ui),
                EditorTab::Players => show_players(app, ui),
                EditorTab::Results => show_results(app, ui),
            }

            ui.separator();

            // ── Save bar ──────────────────────────────────────────────────────
            ui.horizontal(|ui| {
                if ui.button("💾  Save to overrides.json").clicked() {
                    match app.overrides.save(&app.data_dir) {
                        Ok(()) => {
                            app.editor_save_msg = Some(
                                "✓ Saved — re-run:  python pipeline/run.py train".into()
                            );
                            app.editor_save_ok = true;
                        }
                        Err(e) => {
                            app.editor_save_msg = Some(format!("✗ {e}"));
                            app.editor_save_ok = false;
                        }
                    }
                }
                if let Some(ref msg) = app.editor_save_msg {
                    let col = if app.editor_save_ok {
                        Color32::from_rgb(100, 200, 100)
                    } else {
                        Color32::from_rgb(220, 80, 80)
                    };
                    ui.colored_label(col, msg);
                }
            });
        });

    if !open {
        app.editor_open = false;
    }
}

// ── Teams tab ─────────────────────────────────────────────────────────────────

fn show_teams(app: &mut App, ui: &mut Ui) {
    // Snapshot base values from cache so we can borrow overrides mutably below.
    let teams: Vec<(String, f32, f32, f32, f32)> = {
        let mut v: Vec<_> = app.data.teams.iter()
            .map(|t| (t.name.clone(), t.avg_rating, t.recent_wr_1m, t.recent_wr_3m, t.recent_wr_6m))
            .collect();
        v.sort_by_key(|(n, ..)| {
            app.data.team_map.get(n).map(|t| t.ranking).unwrap_or(99)
        });
        v
    };

    ui.label(egui::RichText::new("Drag a value to change it.  Bold = has override.  ● = modified.").weak().italics());
    ui.add_space(4.0);

    let mut to_remove: Option<String> = None;

    ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
        Grid::new("teams_grid")
            .num_columns(8)
            .striped(true)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                // Header
                ui.strong("Team");
                ui.strong("avg_rating");
                ui.strong("wr_1m");
                ui.strong("wr_3m");
                ui.strong("wr_6m");
                ui.label(""); // ● column
                ui.label(""); // × column
                ui.end_row();

                for (name, base_r, base_wr1, base_wr3, base_wr6) in &teams {
                    let entry = app.overrides.teams.entry(name.clone()).or_default();
                    let has_override = !entry.is_empty();

                    if has_override {
                        ui.strong(name);
                    } else {
                        ui.label(name);
                    }

                    stat_drag(ui, &mut entry.avg_rating,   *base_r,    0.70, 1.50);
                    stat_drag(ui, &mut entry.recent_wr_1m, *base_wr1,  0.20, 0.95);
                    stat_drag(ui, &mut entry.recent_wr_3m, *base_wr3,  0.20, 0.95);
                    stat_drag(ui, &mut entry.recent_wr_6m, *base_wr6,  0.20, 0.95);

                    if has_override {
                        ui.colored_label(Color32::from_rgb(100, 210, 100), "●");
                    } else {
                        ui.label("");
                    }

                    if has_override {
                        if ui.small_button("×").on_hover_text("Remove overrides for this team").clicked() {
                            to_remove = Some(name.clone());
                        }
                    } else {
                        ui.label("");
                    }

                    ui.end_row();
                }
            });
    });

    if let Some(name) = to_remove {
        app.overrides.teams.remove(&name);
    }
}

/// Show a DragValue for an `Option<f32>` field; setting it creates/updates
/// the override.  Shows the `base` value when no override is present.
fn stat_drag(ui: &mut Ui, field: &mut Option<f32>, base: f32, min: f32, max: f32) {
    let mut v = field.unwrap_or(base);
    if ui.add(
        DragValue::new(&mut v)
            .speed(0.005)
            .range(min..=max)
            .fixed_decimals(3),
    ).changed() {
        *field = Some(v);
    }
}

// ── Players tab ───────────────────────────────────────────────────────────────

fn show_players(app: &mut App, ui: &mut Ui) {
    // Team selector
    let teams: Vec<String> = {
        let mut v: Vec<_> = app.data.teams.iter().map(|t| t.name.clone()).collect();
        v.sort_by_key(|n| app.data.team_map.get(n).map(|t| t.ranking).unwrap_or(99));
        v
    };

    if app.editor_selected_team.is_empty() {
        if let Some(first) = teams.first() {
            app.editor_selected_team = first.clone();
        }
    }

    ui.horizontal(|ui| {
        ui.label("Team:");
        egui::ComboBox::from_id_source("editor_team_select")
            .selected_text(&app.editor_selected_team)
            .show_ui(ui, |ui| {
                for t in &teams {
                    ui.selectable_value(&mut app.editor_selected_team, t.clone(), t);
                }
            });

        // "Seed from cache" button — creates an override roster from current cache
        let has_override = app.overrides.players.contains_key(&app.editor_selected_team);
        if !has_override {
            if ui.button("📋  Edit roster").on_hover_text(
                "Copy current players from cache into the override so you can edit them."
            ).clicked() {
                let team_name = app.editor_selected_team.clone();
                let seed: Vec<PlayerEntry> = crate::data::team_players(&app.data, &team_name)
                    .into_iter()
                    .map(|p| PlayerEntry {
                        name:       p.name.clone(),
                        rating:     p.rating,
                        kd:         p.kd,
                        adr:        p.adr,
                        kast:       p.kast,
                        od_success: p.opening_duel_success,
                    })
                    .collect();
                app.overrides.players.insert(team_name, seed);
            }
        } else {
            if ui.button("🗑  Clear roster override").clicked() {
                app.overrides.players.remove(&app.editor_selected_team);
            }
        }
    });

    ui.add_space(4.0);
    ui.separator();

    let team_name = app.editor_selected_team.clone();

    if let Some(roster) = app.overrides.players.get_mut(&team_name) {
        // ── Editable override roster ──────────────────────────────────────────
        let mut to_remove: Option<usize> = None;

        ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
            Grid::new("players_grid")
                .num_columns(8)
                .striped(true)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.strong("Name");
                    ui.strong("rating");
                    ui.strong("K/D");
                    ui.strong("ADR");
                    ui.strong("KAST%");
                    ui.strong("OD%");
                    ui.label(""); // delete
                    ui.end_row();

                    for (idx, p) in roster.iter_mut().enumerate() {
                        ui.add(egui::TextEdit::singleline(&mut p.name).desired_width(100.0));
                        ui.add(DragValue::new(&mut p.rating).speed(0.005).range(0.6f32..=1.6f32).fixed_decimals(3));
                        ui.add(DragValue::new(&mut p.kd).speed(0.005).range(0.5f32..=2.0f32).fixed_decimals(3));
                        ui.add(DragValue::new(&mut p.adr).speed(0.5).range(30.0f32..=130.0f32).fixed_decimals(1));
                        ui.add(DragValue::new(&mut p.kast).speed(0.2).range(40.0f32..=95.0f32).fixed_decimals(1));
                        ui.add(DragValue::new(&mut p.od_success).speed(0.002).range(0.3f32..=0.8f32).fixed_decimals(3));
                        if ui.small_button("🗑").clicked() {
                            to_remove = Some(idx);
                        }
                        ui.end_row();
                    }
                });
        });

        if let Some(idx) = to_remove {
            roster.remove(idx);
        }

        ui.add_space(4.0);

        // ── Add player row ────────────────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.label("Add player:");
            ui.add(egui::TextEdit::singleline(&mut app.editor_new_player.name).desired_width(100.0));
            ui.add(DragValue::new(&mut app.editor_new_player.rating).speed(0.005).range(0.6f32..=1.6f32).fixed_decimals(3).prefix("r:"));
            ui.add(DragValue::new(&mut app.editor_new_player.kd).speed(0.005).range(0.5f32..=2.0f32).fixed_decimals(2).prefix("kd:"));
            ui.add(DragValue::new(&mut app.editor_new_player.adr).speed(0.5).range(30.0f32..=130.0f32).fixed_decimals(0).prefix("adr:"));
            if ui.button("➕  Add").clicked() && !app.editor_new_player.name.trim().is_empty() {
                let p = std::mem::replace(&mut app.editor_new_player, PlayerEntry::default());
                if let Some(r) = app.overrides.players.get_mut(&team_name) {
                    r.push(p);
                }
            }
        });
    } else {
        // ── Read-only cache view ──────────────────────────────────────────────
        ui.label(egui::RichText::new("Showing cached data (read-only).  Click \"Edit roster\" above to create an override.").weak().italics());
        ui.add_space(4.0);

        let cached: Vec<_> = crate::data::team_players(&app.data, &team_name)
            .into_iter()
            .map(|p| (p.name.clone(), p.rating, p.kd, p.adr, p.kast, p.opening_duel_success))
            .collect();

        ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
            Grid::new("players_cache_grid")
                .num_columns(6)
                .striped(true)
                .spacing([10.0, 4.0])
                .show(ui, |ui| {
                    ui.strong("Name"); ui.strong("rating"); ui.strong("K/D");
                    ui.strong("ADR"); ui.strong("KAST%"); ui.strong("OD%");
                    ui.end_row();
                    for (name, rating, kd, adr, kast, od) in &cached {
                        ui.label(name);
                        ui.label(format!("{rating:.3}"));
                        ui.label(format!("{kd:.3}"));
                        ui.label(format!("{adr:.1}"));
                        ui.label(format!("{kast:.1}"));
                        ui.label(format!("{od:.3}"));
                        ui.end_row();
                    }
                });
        });
    }
}

// ── Results tab ───────────────────────────────────────────────────────────────

fn show_results(app: &mut App, ui: &mut Ui) {
    ui.label(
        egui::RichText::new(
            "Add match results manually — useful when the live results fetch is blocked by Cloudflare."
        ).weak().italics()
    );
    ui.add_space(4.0);

    let teams: Vec<String> = {
        let mut v: Vec<_> = app.data.teams.iter().map(|t| t.name.clone()).collect();
        v.sort_by_key(|n| app.data.team_map.get(n).map(|t| t.ranking).unwrap_or(99));
        v
    };

    // Seed team dropdowns if empty
    if app.editor_match_a.is_empty() {
        app.editor_match_a = teams.first().cloned().unwrap_or_default();
    }
    if app.editor_match_b.is_empty() {
        app.editor_match_b = teams.get(1).cloned().unwrap_or_default();
    }

    // ── Existing results list ─────────────────────────────────────────────────
    let n = app.overrides.results.matches.len();
    ui.label(format!("{n} manual result(s)"));

    let mut to_remove: Option<usize> = None;

    ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
        Grid::new("results_grid")
            .num_columns(5)
            .striped(true)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                if !app.overrides.results.matches.is_empty() {
                    ui.strong("Team A"); ui.strong("Team B");
                    ui.strong("Winner"); ui.strong("Map"); ui.label("");
                    ui.end_row();
                }
                for (idx, m) in app.overrides.results.matches.iter().enumerate() {
                    let winner_label = if m.winner == m.team_a { "← A" } else { "B →" };
                    ui.label(&m.team_a);
                    ui.label(&m.team_b);
                    ui.label(winner_label);
                    ui.label(m.map.as_deref().unwrap_or("—"));
                    if ui.small_button("🗑").clicked() {
                        to_remove = Some(idx);
                    }
                    ui.end_row();
                }
            });
    });

    if let Some(idx) = to_remove {
        app.overrides.results.matches.remove(idx);
    }

    ui.separator();

    // ── Add new result ────────────────────────────────────────────────────────
    ui.label(egui::RichText::new("Add result:").strong());
    ui.horizontal_wrapped(|ui| {
        ui.label("Team A:");
        egui::ComboBox::from_id_source("match_team_a")
            .selected_text(&app.editor_match_a)
            .width(130.0)
            .show_ui(ui, |ui| {
                for t in &teams {
                    ui.selectable_value(&mut app.editor_match_a, t.clone(), t);
                }
            });

        ui.label("vs  Team B:");
        egui::ComboBox::from_id_source("match_team_b")
            .selected_text(&app.editor_match_b)
            .width(130.0)
            .show_ui(ui, |ui| {
                for t in &teams {
                    ui.selectable_value(&mut app.editor_match_b, t.clone(), t);
                }
            });

        ui.label("Winner:");
        ui.radio_value(&mut app.editor_match_winner_is_a, true,  "A");
        ui.radio_value(&mut app.editor_match_winner_is_a, false, "B");

        ui.label("Map:");
        egui::ComboBox::from_id_source("match_map")
            .selected_text(if app.editor_match_map.is_empty() { "Any" } else { &app.editor_match_map })
            .width(100.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut app.editor_match_map, String::new(), "Any");
                for m in ACTIVE_MAPS {
                    ui.selectable_value(&mut app.editor_match_map, m.to_string(), *m);
                }
            });

        let can_add = app.editor_match_a != app.editor_match_b
            && !app.editor_match_a.is_empty()
            && !app.editor_match_b.is_empty();

        if ui.add_enabled(can_add, egui::Button::new("➕  Add")).clicked() {
            let winner = if app.editor_match_winner_is_a {
                app.editor_match_a.clone()
            } else {
                app.editor_match_b.clone()
            };
            app.overrides.results.matches.push(MatchEntry {
                team_a: app.editor_match_a.clone(),
                team_b: app.editor_match_b.clone(),
                winner,
                map: if app.editor_match_map.is_empty() {
                    None
                } else {
                    Some(app.editor_match_map.clone())
                },
            });
        }
    });
}
