use egui::{Color32, Ui};
use crate::app::App;
use crate::types::Substitute;

// Amber/gold used for stand-in rows so they stand out at a glance.
const SUB_COLOR: Color32 = Color32::from_rgb(255, 185, 50);

pub fn show(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        if ui.button("Reset all").on_hover_text("Clear all form adjustments and stand-ins").clicked() {
            app.form_modifiers.clear();
            app.substitutes.clear();
            app.sub_editing = None;
        }
    });

    ui.add_space(4.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        for team_name in &app.selected_teams.clone() {
            let players = crate::data::team_players(&app.data, team_name);
            if players.is_empty() { continue; }

            // Count active substitutions for this team to show in the header
            let sub_count = players.iter()
                .filter(|p| app.substitutes.contains_key(&p.name))
                .count();
            let header = if sub_count > 0 {
                format!("{team_name}  [{sub_count} sub]")
            } else {
                team_name.clone()
            };

            ui.collapsing(header, |ui| {
                // Column headers
                egui::Grid::new(format!("hdr_{team_name}"))
                    .num_columns(4)
                    .spacing([6.0, 2.0])
                    .show(ui, |ui| {
                        ui.strong("Player");
                        ui.strong("Rtg");
                        ui.strong("Form ±");
                        ui.label("");   // button column
                        ui.end_row();
                    });

                ui.separator();

                // One row per player (plus an optional inline edit row)
                let player_names: Vec<(String, f32)> = players.iter()
                    .map(|p| (p.name.clone(), p.rating))
                    .collect();

                for (orig_name, orig_rating) in &player_names {
                    let is_subbed   = app.substitutes.contains_key(orig_name.as_str());
                    let is_editing  = app.sub_editing.as_deref() == Some(orig_name.as_str());

                    // Resolve display values
                    let (disp_name, disp_rating) = if let Some(sub) = app.substitutes.get(orig_name.as_str()) {
                        (sub.sub_name.clone(), sub.rating)
                    } else {
                        (orig_name.clone(), *orig_rating)
                    };

                    egui::Grid::new(format!("row_{orig_name}"))
                        .num_columns(4)
                        .spacing([6.0, 2.0])
                        .show(ui, |ui| {
                            // Col 1: player name
                            if is_subbed {
                                ui.colored_label(SUB_COLOR, &disp_name)
                                    .on_hover_text(format!("Stand-in for {orig_name}"));
                            } else {
                                ui.label(&disp_name);
                            }

                            // Col 2: effective rating badge
                            let fm = app.form_modifiers.get(orig_name.as_str()).copied().unwrap_or(0.0);
                            let effective = (disp_rating + fm).max(0.5);
                            ui.label(format!("{effective:.2}"));

                            // Col 3: form slider (keyed on original name)
                            let modifier = app.form_modifiers.entry(orig_name.clone()).or_insert(0.0);
                            ui.add(
                                egui::Slider::new(modifier, -0.30_f32..=0.30)
                                    .step_by(0.01)
                                    .fixed_decimals(2)
                                    .show_value(true),
                            );

                            // Col 4: sub / restore button
                            if is_subbed {
                                if ui.small_button("✕ Restore").clicked() {
                                    app.substitutes.remove(orig_name.as_str());
                                    if app.sub_editing.as_deref() == Some(orig_name.as_str()) {
                                        app.sub_editing = None;
                                    }
                                }
                            } else if ui.small_button("↔ Sub").on_hover_text(
                                "Replace this player with a stand-in for the simulation"
                            ).clicked() {
                                // Cancel any other open edit first
                                app.sub_editing  = Some(orig_name.clone());
                                app.sub_edit_name   = orig_name.clone();
                                app.sub_edit_rating = *orig_rating;
                            }

                            ui.end_row();
                        });

                    // ── Inline stand-in editor ────────────────────────────────
                    if is_editing {
                        ui.horizontal(|ui| {
                            ui.label("  →");
                            ui.label("Name:");
                            ui.add_sized(
                                [110.0, 18.0],
                                egui::TextEdit::singleline(&mut app.sub_edit_name),
                            );
                            ui.label("Rtg:");
                            ui.add(
                                egui::DragValue::new(&mut app.sub_edit_rating)
                                    .range(0.50_f32..=2.00)
                                    .speed(0.01)
                                    .fixed_decimals(2),
                            );
                            // Apply
                            if ui.button("✓").on_hover_text("Apply stand-in").clicked() {
                                let sub = Substitute {
                                    sub_name: app.sub_edit_name.trim().to_string(),
                                    rating:   app.sub_edit_rating,
                                };
                                if !sub.sub_name.is_empty() {
                                    app.substitutes.insert(orig_name.clone(), sub);
                                }
                                app.sub_editing = None;
                            }
                            // Cancel
                            if ui.button("✕").on_hover_text("Cancel").clicked() {
                                app.sub_editing = None;
                            }
                        });
                        ui.add_space(2.0);
                    }
                }
            });
        }
    });
}
