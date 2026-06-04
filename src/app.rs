use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{mpsc, Arc};

use crate::data::CachedData;
use crate::inference::InferenceEngine;
use crate::overrides::Overrides;
use crate::presets;
use crate::types::*;
use crate::ui::editor::EditorTab;

pub const STALE_THRESHOLD_DAYS: f32 = 14.0;

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum ResultsTab {
    #[default]
    WinProbs,
    StageReach,
    PlayerPerf,
}

/// Central app state.
pub struct App {
    // ── Loaded data ────────────────────────────────────────────────────────────
    pub data: Arc<CachedData>,
    pub inference: Arc<InferenceEngine>,

    // ── Setup panel ────────────────────────────────────────────────────────────
    pub all_teams: Vec<String>,
    pub selected_teams: Vec<String>,
    pub map_statuses: HashMap<String, MapStatus>,
    pub recency_bias: f32,
    pub mc_runs: u32,

    // ── Tournament builder panel ───────────────────────────────────────────────
    pub format: TournamentFormat,
    pub selected_preset: String,
    pub format_save_path: String,

    // ── Player adjustments / stand-ins panel ─────────────────────────────────
    pub form_modifiers: HashMap<String, f32>,
    /// Active stand-in substitutions: original player name → stand-in data.
    /// Session-only (not persisted to disk).
    pub substitutes: HashMap<String, crate::types::Substitute>,
    /// Which player slot is currently open for stand-in entry (original name).
    pub sub_editing: Option<String>,
    pub sub_edit_name: String,
    pub sub_edit_rating: f32,

    // ── Results panel ─────────────────────────────────────────────────────────
    pub results: Option<SimulationResults>,
    pub results_tab: ResultsTab,
    pub simulation_running: bool,
    result_rx: Option<mpsc::Receiver<SimulationResults>>,

    // ── Data editor ───────────────────────────────────────────────────────────
    pub overrides: Overrides,
    pub editor_open: bool,
    pub editor_tab: EditorTab,
    pub editor_selected_team: String,
    pub editor_new_player: crate::overrides::PlayerEntry,
    pub editor_match_a: String,
    pub editor_match_b: String,
    pub editor_match_winner_is_a: bool,
    pub editor_match_map: String,
    pub editor_save_msg: Option<String>,
    pub editor_save_ok: bool,

    // ── UI ephemeral ───────────────────────────────────────────────────────────
    pub data_dir: PathBuf,
    pub error_message: Option<String>,
    pub show_stale_warning: bool,
    pub show_synthetic_notice: bool,
}

impl App {
    pub fn new(data_dir: PathBuf) -> Self {
        let data = crate::data::load(&data_dir).unwrap_or_else(|e| {
            eprintln!("Data load failed: {e}");
            CachedData {
                teams: vec![],
                players: vec![],
                player_map: HashMap::new(),
                team_map: HashMap::new(),
                cache_age_days: f32::MAX,
                schema: None,
                is_synthetic: true,
                active_maps: ACTIVE_MAPS.iter().map(|s| s.to_string()).collect(),
            }
        });

        let show_stale = data.cache_age_days > STALE_THRESHOLD_DAYS;
        let show_synthetic = data.is_synthetic;
        let overrides = Overrides::load(&data_dir);

        let inference = {
            let schema_ref = data.schema.as_ref();
            Arc::new(InferenceEngine::load(&data_dir, schema_ref))
        };

        let all_teams = crate::data::sorted_teams(&data);
        let selected_teams: Vec<String> = all_teams.iter().take(16).cloned().collect();

        let map_statuses: HashMap<String, MapStatus> = data.active_maps.iter()
            .map(|m| (m.clone(), MapStatus::Active))
            .collect();

        let format = presets::load_preset("CS2 Major").unwrap_or_else(|| TournamentFormat {
            name: "Custom".into(),
            stages: vec![],
        });

        App {
            data: Arc::new(data),
            inference,
            all_teams,
            selected_teams,
            map_statuses,
            recency_bias: 0.5,
            mc_runs: 10_000,
            format,
            selected_preset: "CS2 Major".into(),
            format_save_path: "data/custom_format.json".into(),
            form_modifiers: HashMap::new(),
            substitutes: HashMap::new(),
            sub_editing: None,
            sub_edit_name: String::new(),
            sub_edit_rating: 1.0,
            results: None,
            results_tab: ResultsTab::default(),
            simulation_running: false,
            result_rx: None,
            overrides,
            editor_open: false,
            editor_tab: EditorTab::default(),
            editor_selected_team: String::new(),
            editor_new_player: crate::overrides::PlayerEntry::default(),
            editor_match_a: String::new(),
            editor_match_b: String::new(),
            editor_match_winner_is_a: true,
            editor_match_map: String::new(),
            editor_save_msg: None,
            editor_save_ok: false,
            data_dir: data_dir.clone(),
            error_message: None,
            show_stale_warning: show_stale,
            show_synthetic_notice: show_synthetic,
        }
    }

    /// Called every frame by the egui update loop. Returns true when a new result arrived.
    pub fn poll_simulation(&mut self) -> bool {
        if let Some(ref rx) = self.result_rx {
            match rx.try_recv() {
                Ok(result) => {
                    self.results = Some(result);
                    self.simulation_running = false;
                    self.result_rx = None;
                    return true;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.simulation_running = false;
                    self.result_rx = None;
                    self.error_message = Some("Simulation thread crashed.".into());
                }
            }
        }
        false
    }

    pub fn run_simulation(&mut self, ctx: egui::Context) {
        if self.simulation_running { return; }
        if self.selected_teams.is_empty() {
            self.error_message = Some("No teams selected.".into());
            return;
        }

        let teams: Vec<Team> = self.selected_teams.iter()
            .filter_map(|n| self.data.team_map.get(n).cloned())
            .collect();

        if teams.is_empty() {
            self.error_message = Some("Selected teams not found in cache — run fetch first.".into());
            return;
        }

        let format       = self.format.clone();
        let engine       = Arc::clone(&self.inference);
        let map_statuses = self.map_statuses.clone();
        let form_mods    = self.form_modifiers.clone();
        let subs         = self.substitutes.clone();
        let data         = Arc::clone(&self.data);
        let n_runs       = self.mc_runs;
        let recency      = self.recency_bias;

        let (tx, rx) = mpsc::channel();
        self.result_rx = Some(rx);
        self.simulation_running = true;
        self.error_message = None;

        std::thread::spawn(move || {
            let results = crate::simulation::run_monte_carlo(
                &teams, &format, &engine, &map_statuses, &form_mods, &subs, &data, n_runs, recency,
            );
            let _ = tx.send(results);
            ctx.request_repaint();
        });
    }

    pub fn save_format(&self) {
        if let Ok(json) = serde_json::to_string_pretty(&self.format) {
            let _ = std::fs::write(&self.format_save_path, json);
        }
    }

    pub fn load_format(&mut self) {
        match std::fs::read_to_string(&self.format_save_path)
            .map_err(|e| e.to_string())
            .and_then(|s| serde_json::from_str::<TournamentFormat>(&s).map_err(|e| e.to_string()))
        {
            Ok(fmt) => self.format = fmt,
            Err(e) => self.error_message = Some(format!("Load failed: {e}")),
        }
    }

    pub fn export_results_csv(&self) -> Option<String> {
        let results = self.results.as_ref()?;
        let mut wtr = csv::Writer::from_writer(vec![]);
        let _ = wtr.write_record(["Team", "Win Probability"]);
        let mut sorted: Vec<(&String, &f32)> = results.win_prob.iter().collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));
        for (team, prob) in &sorted {
            let _ = wtr.write_record([team.as_str(), &format!("{:.4}", prob)]);
        }
        wtr.flush().ok()?;
        let inner = wtr.into_inner().ok()?;
        String::from_utf8(inner).ok()
    }
}
