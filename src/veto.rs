use rand::Rng;
use crate::types::{SeriesFormat, Team, MapStatus};

/// Simulate map veto, respecting permabans and team preferences.
/// Returns the ordered list of maps to be played.
pub fn simulate_veto<R: Rng>(
    team_a: &Team,
    team_b: &Team,
    map_statuses: &std::collections::HashMap<String, MapStatus>,
    active_maps: &[String],
    format: SeriesFormat,
    rng: &mut R,
) -> Vec<String> {
    // Start pool = all active maps minus permabanned ones
    let mut pool: Vec<String> = active_maps.iter()
        .filter(|m| map_statuses.get(m.as_str()).copied().unwrap_or(MapStatus::Active) == MapStatus::Active)
        .cloned()
        .collect();

    if pool.is_empty() {
        return vec!["Mirage".to_string()]; // fallback
    }

    match format {
        SeriesFormat::Bo1 => veto_bo1(team_a, team_b, &mut pool, rng),
        SeriesFormat::Bo3 => veto_bo3(team_a, team_b, &mut pool, rng),
        SeriesFormat::Bo5 => veto_bo5(team_a, team_b, &mut pool, rng),
    }
}

// ── Individual veto procedures ────────────────────────────────────────────────

fn veto_bo1<R: Rng>(team_a: &Team, team_b: &Team, pool: &mut Vec<String>, rng: &mut R) -> Vec<String> {
    // Ban Ban Ban Ban Ban Ban → 1 remaining (for 7-map pool)
    let target_size = 1;
    let mut turn = 0usize; // 0 = team_a bans, 1 = team_b bans
    while pool.len() > target_size {
        let banning_team = if turn % 2 == 0 { team_a } else { team_b };
        let idx = worst_map_index(banning_team, pool, rng);
        pool.remove(idx);
        turn += 1;
    }
    pool.clone()
}

fn veto_bo3<R: Rng>(team_a: &Team, team_b: &Team, pool: &mut Vec<String>, rng: &mut R) -> Vec<String> {
    if pool.len() < 3 { return pool.clone(); }
    // Ban Ban Pick Pick Ban Ban → 1 remaining decider
    // Step: A bans, B bans, A picks, B picks, A bans, B bans, remaining = decider
    let mut selected = Vec::new();

    // 2 bans
    for i in 0..2usize {
        let t = if i % 2 == 0 { team_a } else { team_b };
        pool.remove(worst_map_index(t, pool, rng));
    }

    // 2 picks
    for i in 0..2usize {
        if pool.is_empty() { break; }
        let t = if i % 2 == 0 { team_a } else { team_b };
        let idx = best_map_index(t, pool, rng);
        selected.push(pool.remove(idx));
    }

    // 2 more bans
    for i in 0..2usize {
        if pool.len() <= 1 { break; }
        let t = if i % 2 == 0 { team_a } else { team_b };
        pool.remove(worst_map_index(t, pool, rng));
    }

    // Remaining map is decider
    if let Some(decider) = pool.first() {
        selected.push(decider.clone());
    }
    selected
}

fn veto_bo5<R: Rng>(team_a: &Team, team_b: &Team, pool: &mut Vec<String>, rng: &mut R) -> Vec<String> {
    if pool.len() < 5 { return pool.clone(); }
    // Ban Ban Pick Pick Pick Pick → 1 remaining decider
    let mut selected = Vec::new();

    // 2 bans
    for i in 0..2usize {
        if pool.is_empty() { break; }
        let t = if i % 2 == 0 { team_a } else { team_b };
        pool.remove(worst_map_index(t, pool, rng));
    }

    // 4 picks (alternating)
    for i in 0..4usize {
        if pool.is_empty() { break; }
        let t = if i % 2 == 0 { team_a } else { team_b };
        let idx = best_map_index(t, pool, rng);
        selected.push(pool.remove(idx));
    }

    // Remaining = decider
    if let Some(decider) = pool.first() {
        selected.push(decider.clone());
    }
    selected
}

// ── Selection helpers ─────────────────────────────────────────────────────────

/// Index of map the team would most like to ban (worst for them — lowest win rate).
fn worst_map_index<R: Rng>(team: &Team, pool: &[String], rng: &mut R) -> usize {
    let probs = map_ban_probs(team, pool);
    sample_index(&probs, rng)
}

/// Index of map the team would most like to pick (best for them — highest win rate).
fn best_map_index<R: Rng>(team: &Team, pool: &[String], rng: &mut R) -> usize {
    let probs = map_pick_probs(team, pool);
    sample_index(&probs, rng)
}

/// Softmax-weighted ban probabilities (bias toward worst maps).
fn map_ban_probs(team: &Team, pool: &[String]) -> Vec<f64> {
    // Lower map win rate → higher ban probability
    let scores: Vec<f64> = pool.iter()
        .map(|m| 1.0 - team.map_wr(m) as f64)
        .collect();
    softmax(scores, 4.0)
}

/// Softmax-weighted pick probabilities (bias toward best maps).
fn map_pick_probs(team: &Team, pool: &[String]) -> Vec<f64> {
    let scores: Vec<f64> = pool.iter()
        .map(|m| team.map_wr(m) as f64)
        .collect();
    softmax(scores, 4.0)
}

fn softmax(scores: Vec<f64>, temp: f64) -> Vec<f64> {
    let max = scores.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let exps: Vec<f64> = scores.iter().map(|s| ((s - max) * temp).exp()).collect();
    let sum: f64 = exps.iter().sum();
    exps.iter().map(|e| e / sum).collect()
}

fn sample_index<R: Rng>(probs: &[f64], rng: &mut R) -> usize {
    let r: f64 = rng.gen();
    let mut cumulative = 0.0;
    for (i, p) in probs.iter().enumerate() {
        cumulative += p;
        if r < cumulative {
            return i;
        }
    }
    probs.len() - 1
}
