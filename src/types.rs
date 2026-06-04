use std::collections::HashMap;
use serde::{Deserialize, Serialize};

// ── Map constants ─────────────────────────────────────────────────────────────

pub const ACTIVE_MAPS: &[&str] = &[
    "Dust2", "Mirage", "Inferno", "Nuke", "Ancient", "Anubis", "Train",
];

// ── Stand-in substitution ─────────────────────────────────────────────────────

/// Replaces a player with a stand-in for the duration of the simulation session.
/// Stored in `App::substitutes` keyed by the original player name.
#[derive(Clone, Debug)]
pub struct Substitute {
    /// Display name shown in the UI and player-performance output.
    pub sub_name: String,
    /// Absolute rating to use instead of the original player's cached rating.
    pub rating: f32,
}

// ── Team / Player data ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Team {
    pub name: String,
    pub ranking: u32,
    pub avg_rating: f32,
    pub recent_wr_1m: f32,
    pub recent_wr_3m: f32,
    pub recent_wr_6m: f32,
    pub map_pool: Vec<String>,
    pub map_win_rates: HashMap<String, f32>,
    pub h2h: HashMap<String, f32>,
}

impl Team {
    /// Win-rate blended by recency_weight (0 = 6m history, 1 = 1m recent).
    pub fn blended_wr(&self, recency_weight: f32) -> f32 {
        let w = recency_weight.clamp(0.0, 1.0);
        self.recent_wr_6m * (1.0 - w) * 0.5
            + self.recent_wr_3m * (1.0 - w) * 0.5
            + self.recent_wr_1m * w
    }

    pub fn map_wr(&self, map: &str) -> f32 {
        self.map_win_rates.get(map).copied().unwrap_or(0.5)
    }

    pub fn h2h_wr(&self, opponent: &str) -> f32 {
        self.h2h.get(opponent).copied().unwrap_or(0.5)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
    pub name: String,
    pub team: String,
    pub rating: f32,
    pub kd: f32,
    pub adr: f32,
    pub kast: f32,
    pub opening_duel_success: f32,
    pub map_ratings: HashMap<String, f32>,
}

impl Player {
    pub fn map_rating(&self, map: &str) -> f32 {
        self.map_ratings.get(map).copied().unwrap_or(self.rating)
    }
}

// ── Tournament format ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageType {
    Swiss,
    GSL,
    SingleElimination,
    DoubleElimination,
    RoundRobin,
}

impl StageType {
    pub fn label(self) -> &'static str {
        match self {
            Self::Swiss => "Swiss",
            Self::GSL => "GSL",
            Self::SingleElimination => "Single Elimination",
            Self::DoubleElimination => "Double Elimination",
            Self::RoundRobin => "Round Robin",
        }
    }

    pub fn all() -> &'static [StageType] {
        &[
            Self::Swiss,
            Self::GSL,
            Self::SingleElimination,
            Self::DoubleElimination,
            Self::RoundRobin,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SeriesFormat {
    Bo1,
    Bo3,
    Bo5,
}

impl SeriesFormat {
    pub fn label(self) -> &'static str {
        match self {
            Self::Bo1 => "Bo1",
            Self::Bo3 => "Bo3",
            Self::Bo5 => "Bo5",
        }
    }

    pub fn maps_needed(self) -> usize {
        match self {
            Self::Bo1 => 1,
            Self::Bo3 => 3,
            Self::Bo5 => 5,
        }
    }

    pub fn wins_needed(self) -> usize {
        match self {
            Self::Bo1 => 1,
            Self::Bo3 => 2,
            Self::Bo5 => 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoundConfig {
    pub label: String,
    pub format: SeriesFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TournamentStage {
    pub name: String,
    pub stage_type: StageType,
    pub teams_in: usize,
    pub teams_advancing: usize,
    pub round_configs: Vec<RoundConfig>,
}

impl TournamentStage {
    pub fn format_for_round(&self, round_index: usize) -> SeriesFormat {
        self.round_configs
            .get(round_index)
            .or_else(|| self.round_configs.last())
            .map(|rc| rc.format)
            .unwrap_or(SeriesFormat::Bo3)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TournamentFormat {
    pub name: String,
    pub stages: Vec<TournamentStage>,
}

// ── Map pool state ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MapStatus {
    Active,
    Permabanned,
}

// ── Simulation results ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct SimulationResults {
    /// Probability of winning the whole tournament.
    pub win_prob: HashMap<String, f32>,
    /// Probability of reaching (surviving to the end of) each stage.
    /// Key: team name → inner key: stage name → probability.
    pub stage_reach: HashMap<String, HashMap<String, f32>>,
    /// Predicted mean ± std of HLTV rating across simulations.
    pub player_perf: HashMap<String, (f32, f32)>,
}

// ── Single-run intermediates ──────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct RunResult {
    pub champion: String,
    /// stage_name → set of teams that reached/completed that stage
    pub stage_survivors: HashMap<String, Vec<String>>,
    /// player_name → Vec of predicted ratings per map played
    pub player_ratings: HashMap<String, Vec<f32>>,
}
