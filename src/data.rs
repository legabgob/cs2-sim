use std::collections::HashMap;
use std::path::Path;
use anyhow::{Context, Result};
use serde::Deserialize;

use crate::types::{Player, Team};

// ── JSON shapes matching the Python cache ────────────────────────────────────

#[derive(Deserialize)]
struct TeamsFile {
    teams: Vec<Team>,
    #[serde(default)]
    synthetic: bool,
    #[serde(default)]
    active_maps: Vec<String>,
}

#[derive(Deserialize)]
struct PlayersFile {
    players: Vec<Player>,
}

#[derive(Deserialize)]
pub struct FeatureSchema {
    pub match_features: Vec<String>,
    pub player_features: Vec<String>,
    pub match_scaler: ScalerParams,
    pub player_scaler: ScalerParams,
}

#[derive(Deserialize)]
pub struct ScalerParams {
    pub mean: Vec<f32>,
    pub scale: Vec<f32>,
}

impl ScalerParams {
    pub fn transform(&self, x: &[f32]) -> Vec<f32> {
        x.iter()
            .zip(self.mean.iter().zip(self.scale.iter()))
            .map(|(v, (m, s))| (v - m) / s)
            .collect()
    }
}

#[derive(Deserialize)]
struct Metadata {
    #[serde(flatten)]
    entries: HashMap<String, MetaEntry>,
}

#[derive(Deserialize)]
struct MetaEntry {
    updated_at: f64,
}

// ── Public loader ─────────────────────────────────────────────────────────────

pub struct CachedData {
    pub teams: Vec<Team>,
    pub players: Vec<Player>,
    /// Indexed by player name.
    pub player_map: HashMap<String, Player>,
    /// Indexed by team name.
    pub team_map: HashMap<String, Team>,
    /// Cache age in days (oldest of teams/players/matches).
    pub cache_age_days: f32,
    pub schema: Option<FeatureSchema>,
    /// True when no live HLTV data was available and the pipeline fell back to built-in synthetic data.
    pub is_synthetic: bool,
    /// Current CS2 active map pool, written by the Python pipeline.
    /// Falls back to the compile-time ACTIVE_MAPS constant when empty.
    pub active_maps: Vec<String>,
}

pub fn load(data_dir: &Path) -> Result<CachedData> {
    let cache_dir = data_dir.join("cache");

    let teams_json = std::fs::read_to_string(cache_dir.join("teams.json"))
        .context("teams.json not found — run: python pipeline/run.py fetch")?;
    let players_json = std::fs::read_to_string(cache_dir.join("players.json"))
        .context("players.json not found — run: python pipeline/run.py fetch")?;

    let teams_file = serde_json::from_str::<TeamsFile>(&teams_json)?;
    let teams: Vec<Team> = teams_file.teams;
    let is_synthetic = teams_file.synthetic;
    // Use the cached pool if present; fall back to the compile-time constant.
    let active_maps: Vec<String> = if teams_file.active_maps.is_empty() {
        crate::types::ACTIVE_MAPS.iter().map(|s| s.to_string()).collect()
    } else {
        teams_file.active_maps
    };

    let players: Vec<Player> = serde_json::from_str::<PlayersFile>(&players_json)?.players;

    let team_map: HashMap<String, Team> = teams.iter().map(|t| (t.name.clone(), t.clone())).collect();
    let player_map: HashMap<String, Player> = players.iter().map(|p| (p.name.clone(), p.clone())).collect();

    let cache_age_days = read_cache_age(&cache_dir);

    let schema = std::fs::read_to_string(data_dir.join("feature_schema.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());

    Ok(CachedData { teams, players, player_map, team_map, cache_age_days, schema, is_synthetic, active_maps })
}

fn read_cache_age(cache_dir: &Path) -> f32 {
    let meta_path = cache_dir.join("metadata.json");
    let Ok(text) = std::fs::read_to_string(meta_path) else { return f32::MAX };

    // metadata.json has a top-level object where each key is a cache entry with updated_at
    let Ok(val): Result<serde_json::Value, _> = serde_json::from_str(&text) else { return f32::MAX };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);

    let mut oldest = 0.0f64;
    for key in &["teams", "players", "matches"] {
        if let Some(updated) = val.get(key).and_then(|v| v.get("updated_at")).and_then(|v| v.as_f64()) {
            let age = (now - updated) / 86400.0;
            if age > oldest { oldest = age; }
        }
    }
    oldest as f32
}

/// Returns team names sorted by ranking (ascending).
pub fn sorted_teams(data: &CachedData) -> Vec<String> {
    let mut teams = data.teams.clone();
    teams.sort_by_key(|t| t.ranking);
    teams.into_iter().map(|t| t.name).collect()
}

/// Players belonging to a team, sorted by descending rating.
pub fn team_players<'a>(data: &'a CachedData, team: &str) -> Vec<&'a Player> {
    let mut ps: Vec<&Player> = data.players.iter().filter(|p| p.team == team).collect();
    ps.sort_by(|a, b| b.rating.partial_cmp(&a.rating).unwrap_or(std::cmp::Ordering::Equal));
    ps
}

/// Compute team average rating after applying form modifiers and stand-in substitutions.
///
/// For substituted players the stand-in's rating replaces the original base;
/// the form modifier is then added on top (so a "rusty" stand-in can be penalised
/// further with a negative form slider).
pub fn team_avg_rating(
    data: &CachedData,
    team: &str,
    form_modifiers: &HashMap<String, f32>,
    substitutes: &HashMap<String, crate::types::Substitute>,
) -> f32 {
    let players = team_players(data, team);
    if players.is_empty() {
        return data.team_map.get(team).map(|t| t.avg_rating).unwrap_or(1.0);
    }
    let sum: f32 = players.iter().map(|p| {
        let base = substitutes.get(&p.name).map(|s| s.rating).unwrap_or(p.rating);
        let fm   = form_modifiers.get(&p.name).copied().unwrap_or(0.0);
        (base + fm).max(0.5)
    }).sum();
    sum / players.len() as f32
}
